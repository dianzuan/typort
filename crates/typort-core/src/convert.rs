use std::collections::BTreeMap;

use typort_ooxml::document::{
    Alignment, BlockElement, Document, Paragraph, ParagraphStyle, Run, Table, TableCell, TableRow,
};
use typst::foundations::{Content, NativeElement};
use typst::introspection::Tag;
use typst::layout::{Frame, FrameItem, PagedDocument, Point};
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;

use crate::world::TyportWorld;

/// Convert a Typst source file to an OOXML `Document` via the `HtmlDocument` semantic DOM.
///
/// # Errors
/// Returns compilation errors if the Typst source cannot be compiled.
pub fn convert_html(world: &TyportWorld) -> Result<Document, Vec<String>> {
    let result = typst::compile::<HtmlDocument>(world);
    let html_doc = match result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };

    // Query all equations via introspector (ordered by location)
    let equations = html_doc.introspector.query(&EquationElem::ELEM.select());

    let mut doc = Document::new();
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);

    // First pass: extract footnote content from <section role="doc-endnotes">
    let footnote_contents = extract_footnote_contents(&body.children);

    // Register all footnotes in the document
    for content in &footnote_contents {
        doc.add_footnote(content.clone());
    }

    let mut eq_counter = 0usize;
    convert_block_children_with_math(&body.children, &mut doc, &equations, &mut eq_counter);

    // Recovery: compile also to PagedDocument and recover content that was lost
    // (e.g. #align(center) content which has no HTML show rule)
    recover_missing_content(world, &mut doc);

    // Extract title from the first heading element
    extract_title_from_first_heading(&mut doc);

    Ok(doc)
}

fn find_body(root: &HtmlElement) -> Option<&HtmlElement> {
    for child in &root.children {
        if let HtmlNode::Element(elem) = child {
            let tag = tag_name(elem);
            if tag == "body" {
                return Some(elem);
            }
            if let Some(found) = find_body(elem) {
                return Some(found);
            }
        }
    }
    None
}

fn tag_name(elem: &HtmlElement) -> String {
    let raw = format!("{}", elem.tag);
    raw.trim_matches('<').trim_matches('>').to_string()
}

fn convert_block_children_with_math(
    children: &[HtmlNode],
    doc: &mut Document,
    equations: &[Content],
    eq_counter: &mut usize,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Element(elem) => {
                convert_element_with_math(elem, doc, equations, eq_counter);
            }
            HtmlNode::Tag(tag) => {
                if is_tag_start(tag, "equation") {
                    // Convert the equation using the introspector data
                    if let Some(eq_content) = equations.get(*eq_counter) {
                        let omml = typort_math::equation_to_omml(eq_content);

                        // Check if this is a block equation
                        let is_block = eq_content
                            .to_packed::<EquationElem>()
                            .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));

                        if is_block {
                            // Block equation gets its own paragraph
                            let mut para = Paragraph::new();
                            para.add_math(omml);
                            doc.add_paragraph(para);
                        } else if let Some(typort_ooxml::document::BlockElement::Paragraph(para)) =
                            doc.body.elements.last_mut()
                        {
                            // Inline equation: attach to the last paragraph
                            para.add_math(omml);
                        } else {
                            let mut para = Paragraph::new();
                            para.add_math(omml);
                            doc.add_paragraph(para);
                        }
                    }
                    *eq_counter += 1;
                    // Skip to the matching End tag
                    let start_loc = tag.location();
                    i += 1;
                    while i < children.len() {
                        if let HtmlNode::Tag(end_tag) = &children[i]
                            && is_tag_end_for(end_tag, start_loc)
                        {
                            break;
                        }
                        i += 1;
                    }
                }
            }
            HtmlNode::Frame(_) => {}
            HtmlNode::Text(text, _) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.runs.push(Run::new(trimmed));
                    doc.add_paragraph(para);
                }
            }
        }
        i += 1;
    }
}

fn convert_element_with_math(
    elem: &HtmlElement,
    doc: &mut Document,
    equations: &[Content],
    eq_counter: &mut usize,
) {
    let tag = tag_name(elem);
    match tag.as_str() {
        "h2" => convert_heading(elem, doc, 1),
        "h3" => convert_heading(elem, doc, 2),
        "h4" => convert_heading(elem, doc, 3),
        "h5" => convert_heading(elem, doc, 4),
        "h6" => convert_heading(elem, doc, 5),
        "p" => convert_paragraph(elem, doc),
        "ol" | "ul" => convert_list(elem, doc),
        "table" => convert_table(elem, doc),
        "div" => {
            // Check if this div has alignment styling
            if detect_alignment(elem).is_some() {
                convert_paragraph(elem, doc);
            } else {
                convert_block_children_with_math(&elem.children, doc, equations, eq_counter);
            }
        }
        "section" => {
            // Skip the doc-endnotes section (already extracted)
            if has_attr_value(elem, "role", "doc-endnotes") {
                return;
            }
            convert_block_children_with_math(&elem.children, doc, equations, eq_counter);
        }
        _ => convert_block_children_with_math(&elem.children, doc, equations, eq_counter),
    }
}

fn convert_heading(elem: &HtmlElement, doc: &mut Document, level: u8) {
    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level));
    collect_inlines(&elem.children, &mut para, false, false, doc);
    doc.add_paragraph(para);
}

fn convert_paragraph(elem: &HtmlElement, doc: &mut Document) {
    let mut para = Paragraph::new();
    para.alignment = detect_alignment(elem);
    collect_inlines(&elem.children, &mut para, false, false, doc);
    if !para.runs.is_empty() {
        doc.add_paragraph(para);
    }
}

/// Detect text alignment from the `style` attribute of an element.
/// Typst's `#align(center)` produces elements with style containing "text-align: center".
fn detect_alignment(elem: &HtmlElement) -> Option<Alignment> {
    let style_val = get_attr_value(elem, "style")?;
    if style_val.contains("text-align: center") || style_val.contains("text-align:center") {
        Some(Alignment::Center)
    } else if style_val.contains("text-align: right") || style_val.contains("text-align:right") {
        Some(Alignment::Right)
    } else {
        None
    }
}

/// Get the value of an attribute by name.
fn get_attr_value(elem: &HtmlElement, attr_name: &str) -> Option<String> {
    for (k, v) in &elem.attrs.0 {
        let key_str = format!("{k}");
        if key_str == attr_name {
            return Some(format!("{v}"));
        }
    }
    None
}

fn convert_list(elem: &HtmlElement, doc: &mut Document) {
    let tag = tag_name(elem);
    // list_id: 1 for ordered, 2 for unordered
    let list_id = if tag == "ol" { 1 } else { 2 };
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            para.list_id = Some(list_id);
            para.list_level = Some(0);
            collect_inlines(&li.children, &mut para, false, false, doc);
            if !para.runs.is_empty() {
                doc.add_paragraph(para);
            }
        }
    }
}

fn convert_table(elem: &HtmlElement, doc: &mut Document) {
    let mut table = Table { rows: Vec::new() };
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                if let Some(row) = convert_table_row(row_or_section, doc) {
                    table.rows.push(row);
                }
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                        && let Some(row) = convert_table_row(tr, doc)
                    {
                        table.rows.push(row);
                    }
                }
            }
        }
    }
    if !table.rows.is_empty() {
        doc.add_table(table);
    }
}

fn convert_table_row(tr: &HtmlElement, doc: &Document) -> Option<TableRow> {
    let mut cells = Vec::new();
    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                let mut para = Paragraph::new();
                collect_inlines(&td.children, &mut para, tag == "th", false, doc);
                cells.push(TableCell {
                    paragraphs: vec![para],
                });
            }
        }
    }
    if cells.is_empty() {
        None
    } else {
        Some(TableRow { cells })
    }
}

/// Collect inline elements from children, tracking footnote boundaries via Tag markers.
///
/// The Typst HTML DOM represents footnotes as:
///   TAG Start("footnote") -> <a role="doc-noteref"><sup>N</sup></a> -> TAG End(loc)
///
/// We detect the footnote start tag and skip the inline content of the reference
/// (since Word handles rendering the superscript number), inserting a `FootnoteRef` instead.
#[allow(clippy::only_used_in_recursion)]
fn collect_inlines(
    children: &[HtmlNode],
    para: &mut Paragraph,
    bold: bool,
    italic: bool,
    doc: &Document,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    run.italic = italic;
                    para.push_run(run);
                }
            }
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                // Skip the doc-noteref anchor (footnote reference link) - we handle
                // footnotes via the Tag markers instead.
                if tag.contains('a') && has_attr_value(elem, "role", "doc-noteref") {
                    // Skip - handled by the Tag("footnote") marker
                } else {
                    let new_bold = bold || tag == "strong" || tag == "b";
                    let new_italic = italic || tag == "em" || tag == "i";
                    collect_inlines(&elem.children, para, new_bold, new_italic, doc);
                }
            }
            HtmlNode::Tag(tag) => {
                if is_tag_start(tag, "footnote") {
                    // Get the location so we can find the matching End tag
                    let start_loc = tag.location();
                    // Find the footnote number by looking at the next children.
                    let footnote_id = find_footnote_id_in_range(&children[i..]);
                    if let Some(id) = footnote_id {
                        para.add_footnote_ref(id);
                    }
                    // Skip ahead past the matching TAG End (same location)
                    i += 1;
                    while i < children.len() {
                        if let HtmlNode::Tag(end_tag) = &children[i]
                            && is_tag_end_for(end_tag, start_loc)
                        {
                            break;
                        }
                        i += 1;
                    }
                }
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

/// Check if a `Tag` is a Start tag with the given element name.
fn is_tag_start(tag: &Tag, name: &str) -> bool {
    if let Tag::Start(content, _) = tag {
        content.elem().name() == name
    } else {
        false
    }
}

/// Check if a `Tag` is the End tag matching a given start location.
fn is_tag_end_for(tag: &Tag, start_loc: typst::introspection::Location) -> bool {
    if let Tag::End(loc, ..) = tag {
        *loc == start_loc
    } else {
        false
    }
}

/// Check if an element has a specific attribute value.
fn has_attr_value(elem: &HtmlElement, attr_name: &str, attr_value: &str) -> bool {
    elem.attrs.0.iter().any(|(k, v)| {
        let key_str = format!("{k}");
        let val_str = format!("{v}");
        key_str == attr_name && val_str == attr_value
    })
}

/// Find the footnote number from the children starting at a TAG Start("footnote").
/// Looks for <a role="doc-noteref"> -> <sup> -> text number.
fn find_footnote_id_in_range(children: &[HtmlNode]) -> Option<u32> {
    for child in children {
        match child {
            HtmlNode::Element(elem) => {
                if has_attr_value(elem, "role", "doc-noteref") {
                    // Look for <sup> -> text inside
                    return find_sup_number(&elem.children);
                }
                // Recurse slightly
                if let Some(id) = find_footnote_id_in_range(&elem.children) {
                    return Some(id);
                }
            }
            HtmlNode::Tag(tag) => {
                // If we hit an End tag, we've gone past the reference area
                if matches!(tag, Tag::End(..)) {
                    break;
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a number inside <sup> elements.
fn find_sup_number(children: &[HtmlNode]) -> Option<u32> {
    for child in children {
        if let HtmlNode::Element(elem) = child {
            let tag = tag_name(elem);
            if tag == "sup"
                && let Some(text) = get_text_content(&elem.children)
            {
                return text.trim().parse().ok();
            }
            if let Some(n) = find_sup_number(&elem.children) {
                return Some(n);
            }
        }
    }
    None
}

/// Get concatenated text content from children.
fn get_text_content(children: &[HtmlNode]) -> Option<String> {
    let mut text = String::new();
    for child in children {
        if let HtmlNode::Text(t, _) = child {
            text.push_str(t);
        }
    }
    if text.is_empty() { None } else { Some(text) }
}

/// Extract footnote content from the <section role="doc-endnotes"> at the end of body.
/// Returns a Vec of footnote content (each as a Vec<Run>), in order (1-based index).
fn extract_footnote_contents(children: &[HtmlNode]) -> Vec<Vec<Run>> {
    let mut footnotes = Vec::new();

    // Find the <section role="doc-endnotes"> element
    for child in children {
        if let HtmlNode::Element(elem) = child
            && tag_name(elem) == "section"
            && has_attr_value(elem, "role", "doc-endnotes")
        {
            // Inside: <ol> -> <li id="loc-N"> items
            for ol_child in &elem.children {
                if let HtmlNode::Element(ol) = ol_child
                    && tag_name(ol) == "ol"
                {
                    extract_footnotes_from_ol(&ol.children, &mut footnotes);
                }
            }
        }
    }

    footnotes
}

/// Extract footnote content from <li> elements inside the <ol>.
fn extract_footnotes_from_ol(children: &[HtmlNode], footnotes: &mut Vec<Vec<Run>>) {
    for child in children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            // Collect the text content, skipping the backlink <a> and <sup>
            let mut runs = Vec::new();
            collect_footnote_text(&li.children, &mut runs);
            footnotes.push(runs);
        }
    }
}

/// Collect text from footnote <li> content, skipping the backlink anchor.
fn collect_footnote_text(children: &[HtmlNode], runs: &mut Vec<Run>) {
    // The structure is:
    //   TAG Start("entry")
    //   TAG Start("link")
    //   <a role="doc-backlink"><sup>N</sup></a>
    //   TAG End("link")
    //   TEXT "content..."
    //   TAG End("entry")
    // We want just the text nodes, skipping the backlink.
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    runs.push(Run::new(text.as_str()));
                }
            }
            HtmlNode::Element(elem) => {
                // Skip backlink anchors
                if has_attr_value(elem, "role", "doc-backlink") {
                    continue;
                }
                collect_footnote_text(&elem.children, runs);
            }
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
}

/// Extract title from the first heading paragraph and set it as document metadata.
fn extract_title_from_first_heading(doc: &mut Document) {
    use typort_ooxml::document::{BlockElement, ParagraphStyle};

    for element in &doc.body.elements {
        if let BlockElement::Paragraph(p) = element
            && matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            let title: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            if !title.is_empty() {
                doc.metadata.title = Some(title);
            }
            break;
        }
    }
}

/// Recover content that exists in the `PagedDocument` but was lost from the `HtmlDocument` DOM.
///
/// This handles the case where `#align(center)[...]` content completely vanishes from the
/// HTML semantic output (no show rule exists for `AlignElem` in Typst's HTML target). We detect
/// the missing text by comparing the paged output with our converted document, then insert
/// the missing lines as centered paragraphs at the appropriate position.
fn recover_missing_content(world: &TyportWorld, doc: &mut Document) {
    let paged_result = typst::compile::<PagedDocument>(world);
    let Ok(paged) = paged_result.output else {
        return;
    };

    // Only look at the FIRST PAGE for title-area recovery (author/institution info).
    // Other pages' differences (math as OMML, footnotes in footnotes.xml) are NOT missing.
    let first_page_lines = extract_lines_from_first_page(&paged);
    if first_page_lines.is_empty() {
        return;
    }

    // Find where the title headings end in the document
    let title_end_idx = find_title_section_end(doc);

    // Extract text that appears in our document's title section
    let title_section_text = extract_text_around_title(doc, title_end_idx);

    // Only recover lines that:
    // 1. Appear BETWEEN the title and the first body content on page 1
    // 2. Are NOT already in our document
    // 3. Look like author/institution info (heuristic: positioned after title, before body)
    let title_line_count = count_title_lines(&first_page_lines, doc);
    let body_start_line = find_body_start_line(&first_page_lines, doc);

    let mut missing = Vec::new();
    for (i, line) in first_page_lines.iter().enumerate() {
        if i < title_line_count || i >= body_start_line {
            continue;
        }
        if line.text.chars().count() < 2 {
            continue;
        }
        if !title_section_text.contains(&line.text) && !extract_doc_text(doc).contains(&line.text) {
            missing.push(line.clone());
        }
    }

    if !missing.is_empty() {
        insert_missing_at_position(doc, &missing);
    }
}

fn find_title_section_end(doc: &Document) -> usize {
    for (i, elem) in doc.body.elements.iter().enumerate() {
        if let BlockElement::Paragraph(p) = elem {
            if !matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                return i;
            }
        } else {
            return i;
        }
    }
    doc.body.elements.len()
}

fn extract_text_around_title(doc: &Document, title_end: usize) -> String {
    let mut text = String::new();
    let end = (title_end + 3).min(doc.body.elements.len());
    for elem in &doc.body.elements[..end] {
        if let BlockElement::Paragraph(p) = elem {
            for run in &p.runs {
                text.push_str(&run.text);
            }
        }
    }
    text
}

fn count_title_lines(paged_lines: &[FrameLine], doc: &Document) -> usize {
    let mut count = 0;
    for line in paged_lines {
        let is_heading = doc.body.elements.iter().any(|e| {
            if let BlockElement::Paragraph(p) = e
                && matches!(p.style, Some(ParagraphStyle::Heading(_)))
            {
                p.runs.iter().any(|r| line.text.contains(&r.text))
            } else {
                false
            }
        });
        if is_heading {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn find_body_start_line(paged_lines: &[FrameLine], doc: &Document) -> usize {
    let first_body_text = doc.body.elements.iter().find_map(|e| {
        if let BlockElement::Paragraph(p) = e
            && !matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            p.runs.first().map(|r| r.text.clone())
        } else {
            None
        }
    });

    if let Some(body_text) = first_body_text {
        for (i, line) in paged_lines.iter().enumerate() {
            if line.text.contains(&body_text[..body_text.len().min(10)]) {
                return i;
            }
        }
    }
    paged_lines.len()
}

/// A text line extracted from a `PagedDocument` frame, with its Y position (for ordering).
#[derive(Debug, Clone)]
struct FrameLine {
    /// The Y position on the page (in typographic points), used for ordering.
    #[allow(dead_code)]
    y_pos: f64,
    /// The concatenated text of all runs on this line.
    text: String,
}

/// Extract text lines from the first page of a `PagedDocument`.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn extract_lines_from_first_page(paged: &PagedDocument) -> Vec<FrameLine> {
    let mut all_lines = Vec::new();
    for page in paged.pages.iter().take(1) {
        let mut text_items: Vec<(f64, f64, String)> = Vec::new(); // (y, x, text)
        collect_text_items_with_pos(&page.frame, Point::zero(), &mut text_items);

        // Group by Y coordinate (tolerance of 2pt for same-line items)
        let mut y_groups: BTreeMap<i64, Vec<(f64, String)>> = BTreeMap::new();
        for (y, x, text) in text_items {
            // Quantize Y to nearest 2pt for grouping
            let y_key = (y / 2.0).round() as i64;
            y_groups.entry(y_key).or_default().push((x, text));
        }

        // Sort each group by X and concatenate to form the line text
        for (y_key, mut items) in y_groups {
            items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let line_text: String = items.into_iter().map(|(_, t)| t).collect();
            let trimmed = line_text.trim().to_string();
            if !trimmed.is_empty() {
                all_lines.push(FrameLine {
                    y_pos: y_key as f64 * 2.0,
                    text: trimmed,
                });
            }
        }
    }
    all_lines
}

/// Recursively collect text items from a frame with their absolute positions.
fn collect_text_items_with_pos(frame: &Frame, offset: Point, items: &mut Vec<(f64, f64, String)>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if !text.is_empty() {
                    items.push((abs_y.to_pt(), abs_x.to_pt(), text));
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_text_items_with_pos(&group.frame, new_offset, items);
            }
            _ => {}
        }
    }
}

/// Extract all text from the Document model as a single normalized string.
fn extract_doc_text(doc: &Document) -> String {
    let mut text = String::new();
    for elem in &doc.body.elements {
        match elem {
            BlockElement::Paragraph(p) => {
                for run in &p.runs {
                    text.push_str(&run.text);
                }
            }
            BlockElement::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for para in &cell.paragraphs {
                            for run in &para.runs {
                                text.push_str(&run.text);
                            }
                        }
                    }
                }
            }
        }
    }
    text
}

/// Insert missing lines as centered paragraphs at the correct position in the document.
///
/// The insertion point is after the last consecutive heading at the start of the document,
/// before the first non-heading paragraph. This matches the typical academic paper structure:
/// - Title heading
/// - Subtitle heading (optional)
/// - [INSERT HERE: author/institution info]
/// - Abstract paragraph
/// - Body content
fn insert_missing_at_position(doc: &mut Document, missing_lines: &[FrameLine]) {
    // Find the insertion point: after the last consecutive heading at the start
    let mut insert_idx = 0;
    for (i, elem) in doc.body.elements.iter().enumerate() {
        if let BlockElement::Paragraph(p) = elem {
            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                insert_idx = i + 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Build centered paragraphs for each missing line
    let mut paragraphs: Vec<BlockElement> = Vec::new();
    for line in missing_lines {
        let mut para = Paragraph::new();
        para.alignment = Some(Alignment::Center);
        para.add_run(&line.text);
        paragraphs.push(BlockElement::Paragraph(para));
    }

    // Insert at the computed position
    if !paragraphs.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(paragraphs);
        doc.body.elements.extend(tail);
    }
}

/// Legacy Phase 0 converter (frame-based text extraction).
pub mod legacy {
    use typort_ooxml::document::{Document, Paragraph};
    use typst::layout::{Frame, FrameItem, PagedDocument};

    #[must_use]
    pub fn convert_document(paged: &PagedDocument) -> Document {
        let mut doc = Document::new();
        for page in &paged.pages {
            let mut text_buf = String::new();
            extract_text_from_frame(&page.frame, &mut text_buf);
            if !text_buf.is_empty() {
                let mut para = Paragraph::new();
                para.add_run(&text_buf);
                doc.add_paragraph(para);
            }
        }
        doc
    }

    fn extract_text_from_frame(frame: &Frame, buf: &mut String) {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Text(text_item) => buf.push_str(&text_item.text),
                FrameItem::Group(group) => extract_text_from_frame(&group.frame, buf),
                _ => {}
            }
        }
    }
}
