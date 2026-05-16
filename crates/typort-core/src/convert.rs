use std::collections::{BTreeMap, HashMap};

use typort_ooxml::document::{
    Alignment, BlockElement, Document, DocumentStyle, InlineElement, Paragraph, ParagraphStyle,
    Run, Table, TableCell, TableRow, VMerge,
};
use typst::foundations::{Content, NativeElement};
use typst::introspection::Tag;
use typst::layout::{Frame, FrameItem, PagedDocument, Point};
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;
use typst_library::model::Numbering;

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

    let mut eq_state = EquationState::default();
    let mut para_ctx = ParagraphContext::default();
    convert_block_children_with_math(
        &body.children,
        &mut doc,
        &equations,
        &mut eq_state,
        &mut para_ctx,
    );

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

#[derive(Default)]
struct EquationState {
    eq_index: usize,
    chapter: u64,
    eq_in_chapter: u64,
    global_eq: u64,
}

/// Tracks context for paragraph formatting decisions.
#[derive(Default)]
struct ParagraphContext {
    /// The next non-heading paragraph should suppress first-line indent.
    after_heading: bool,
    /// We are inside the bibliography section (after "参考文献" heading).
    in_bibliography: bool,
}

fn tag_name(elem: &HtmlElement) -> String {
    let raw = format!("{}", elem.tag);
    raw.trim_matches('<').trim_matches('>').to_string()
}

#[allow(clippy::too_many_lines)]
fn convert_block_children_with_math(
    children: &[HtmlNode],
    doc: &mut Document,
    equations: &[Content],
    eq_state: &mut EquationState,
    para_ctx: &mut ParagraphContext,
) {
    let mut i = 0;
    // Track whether the last item was an inline equation — if so, the next <p>
    // should merge into the current paragraph rather than starting a new one.
    let mut continue_paragraph = false;

    while i < children.len() {
        match &children[i] {
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                if tag == "p" && continue_paragraph {
                    // Merge into current paragraph using a temp Paragraph to avoid borrow conflict
                    let mut tmp = Paragraph::new();
                    collect_inlines(&elem.children, &mut tmp, false, false);
                    if let Some(BlockElement::Paragraph(para)) = doc.body.elements.last_mut() {
                        for inline in tmp.inlines {
                            if let InlineElement::Text(ref run) = inline {
                                para.runs.push(run.clone());
                            }
                            para.inlines.push(inline);
                        }
                    }
                    continue_paragraph = false;
                } else {
                    continue_paragraph = false;
                    convert_element_with_math(elem, doc, equations, eq_state, para_ctx);
                }
            }
            HtmlNode::Tag(tag) => {
                if is_tag_start(tag, "equation") {
                    if let Some(eq_content) = equations.get(eq_state.eq_index) {
                        let omml = typort_math::equation_to_omml(eq_content);

                        let eq_packed = eq_content.to_packed::<EquationElem>();
                        let is_block = eq_packed
                            .as_ref()
                            .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));

                        let eq_number = if is_block {
                            eq_packed.as_ref().and_then(|eq| {
                                let numbering_opt = eq.numbering.as_option().as_ref()?.as_ref()?;
                                if let Numbering::Pattern(pattern) = numbering_opt {
                                    eq_state.global_eq += 1;
                                    eq_state.eq_in_chapter += 1;
                                    let pieces = pattern.pieces();
                                    let nums: Vec<u64> = if pieces >= 2 {
                                        vec![eq_state.chapter, eq_state.eq_in_chapter]
                                    } else {
                                        vec![eq_state.global_eq]
                                    };
                                    Some(pattern.apply(&nums).to_string())
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        };

                        if is_block {
                            let mut para = Paragraph::new();
                            if let Some(number) = eq_number {
                                para.add_numbered_math(omml, number);
                            } else {
                                para.add_math(omml);
                            }
                            doc.add_paragraph(para);
                            continue_paragraph = false;
                        } else if let Some(BlockElement::Paragraph(para)) =
                            doc.body.elements.last_mut()
                        {
                            para.add_math(omml);
                            continue_paragraph = true;
                        } else {
                            let mut para = Paragraph::new();
                            para.add_math(omml);
                            doc.add_paragraph(para);
                            continue_paragraph = true;
                        }
                    }
                    eq_state.eq_index += 1;
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
                } else {
                    // Non-equation tags don't reset continue_paragraph
                }
            }
            HtmlNode::Frame(_) => {
                // Frame elements at block level may contain images.
                // Emit a placeholder paragraph.
                let mut para = Paragraph::new();
                let mut run = Run::new("[Image]");
                run.italic = true;
                para.push_run(run);
                doc.add_paragraph(para);
                continue_paragraph = false;
            }
            HtmlNode::Text(text, _) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if continue_paragraph {
                        if let Some(BlockElement::Paragraph(para)) = doc.body.elements.last_mut() {
                            para.push_run(Run::new(trimmed));
                        }
                    } else {
                        let mut para = Paragraph::new();
                        para.runs.push(Run::new(trimmed));
                        doc.add_paragraph(para);
                    }
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
    eq_state: &mut EquationState,
    para_ctx: &mut ParagraphContext,
) {
    let tag = tag_name(elem);
    match tag.as_str() {
        "h2" => {
            eq_state.chapter += 1;
            eq_state.eq_in_chapter = 0;
            convert_heading(elem, doc, 1, para_ctx);
        }
        "h3" => convert_heading(elem, doc, 2, para_ctx),
        "h4" => convert_heading(elem, doc, 3, para_ctx),
        "h5" => convert_heading(elem, doc, 4, para_ctx),
        "h6" => convert_heading(elem, doc, 5, para_ctx),
        "p" => convert_paragraph(elem, doc, para_ctx),
        "pre" => convert_code_block(elem, doc),
        "blockquote" => convert_blockquote(elem, doc, equations, eq_state, para_ctx),
        "dl" => convert_term_list(elem, doc),
        "ol" | "ul" => convert_list(elem, doc),
        "table" => convert_table(elem, doc),
        "div" => {
            // Check if this div has alignment styling
            if detect_alignment(elem).is_some() {
                convert_paragraph(elem, doc, para_ctx);
            } else {
                convert_block_children_with_math(
                    &elem.children,
                    doc,
                    equations,
                    eq_state,
                    para_ctx,
                );
            }
        }
        "section" => {
            // Skip the doc-endnotes section (already extracted)
            if has_attr_value(elem, "role", "doc-endnotes") {
                return;
            }
            convert_block_children_with_math(&elem.children, doc, equations, eq_state, para_ctx);
        }
        _ => convert_block_children_with_math(&elem.children, doc, equations, eq_state, para_ctx),
    }
}

fn convert_heading(
    elem: &HtmlElement,
    doc: &mut Document,
    level: u8,
    para_ctx: &mut ParagraphContext,
) {
    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level));
    collect_inlines(&elem.children, &mut para, false, false);

    // Detect bibliography section by heading text
    let heading_text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
    if heading_text.contains("参考文献") {
        para_ctx.in_bibliography = true;
    }

    doc.add_paragraph(para);
    para_ctx.after_heading = true;
}

fn convert_paragraph(elem: &HtmlElement, doc: &mut Document, para_ctx: &mut ParagraphContext) {
    let mut para = Paragraph::new();
    para.alignment = detect_alignment(elem);
    collect_inlines(&elem.children, &mut para, false, false);
    if !para.runs.is_empty() {
        // Feature 3: suppress indent on first paragraph after heading
        if para_ctx.after_heading && para.style.is_none() {
            para.suppress_indent = true;
        }
        // Feature 4: bibliography hanging indent
        if para_ctx.in_bibliography && para.style.is_none() {
            para.hanging_indent = true;
        }
        para_ctx.after_heading = false;
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
            collect_inlines(&li.children, &mut para, false, false);
            if !para.runs.is_empty() {
                doc.add_paragraph(para);
            }
        }
    }
}

/// Convert a `<pre>` code block into monospace paragraphs (one per line).
fn convert_code_block(elem: &HtmlElement, doc: &mut Document) {
    // Collect all text content from the <pre> (typically contains a single <code>)
    let text = collect_all_text(&elem.children);
    // Split on newlines to produce one paragraph per line
    for line in text.split('\n') {
        let mut para = Paragraph::new();
        para.code_block = true;
        let mut run = Run::new(line);
        run.monospace = true;
        para.push_run(run);
        doc.add_paragraph(para);
    }
}

/// Convert a `<blockquote>` into indented paragraphs.
fn convert_blockquote(
    elem: &HtmlElement,
    doc: &mut Document,
    equations: &[Content],
    eq_state: &mut EquationState,
    para_ctx: &mut ParagraphContext,
) {
    // Save the current element count so we can apply left_indent to newly added paragraphs
    let start_idx = doc.body.elements.len();
    convert_block_children_with_math(&elem.children, doc, equations, eq_state, para_ctx);
    // Apply left indent to all paragraphs added by the blockquote
    for element in &mut doc.body.elements[start_idx..] {
        if let BlockElement::Paragraph(para) = element {
            para.left_indent = Some(720);
            para.suppress_indent = true;
        }
    }
}

/// Convert a `<dl>` (definition list) into bold terms and indented definitions.
fn convert_term_list(elem: &HtmlElement, doc: &mut Document) {
    for child in &elem.children {
        if let HtmlNode::Element(item) = child {
            let tag = tag_name(item);
            match tag.as_str() {
                "dt" => {
                    let mut para = Paragraph::new();
                    para.suppress_indent = true;
                    collect_inlines(&item.children, &mut para, true, false);
                    if !para.runs.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                "dd" => {
                    let mut para = Paragraph::new();
                    para.left_indent = Some(420);
                    para.suppress_indent = true;
                    collect_inlines(&item.children, &mut para, false, false);
                    if !para.runs.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Recursively collect all text content from a node tree.
fn collect_all_text(children: &[HtmlNode]) -> String {
    let mut text = String::new();
    let mut line_started = false;
    for child in children {
        match child {
            HtmlNode::Text(t, _) => text.push_str(t),
            HtmlNode::Element(elem) => text.push_str(&collect_all_text(&elem.children)),
            HtmlNode::Tag(tag) => {
                if is_tag_start(tag, "line") {
                    if line_started {
                        text.push('\n');
                    }
                    line_started = true;
                }
            }
            HtmlNode::Frame(_) => {}
        }
    }
    text
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

fn convert_table_row(tr: &HtmlElement, _doc: &Document) -> Option<TableRow> {
    let mut cells = Vec::new();
    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                let mut para = Paragraph::new();
                collect_inlines(&td.children, &mut para, tag == "th", false);

                // Parse colspan and rowspan attributes
                let colspan = get_attr_value(td, "colspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let rowspan = get_attr_value(td, "rowspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);

                let vmerge = if rowspan > 1 {
                    VMerge::Restart
                } else {
                    VMerge::None
                };

                cells.push(TableCell {
                    paragraphs: vec![para],
                    colspan,
                    vmerge,
                    width_pct: None,
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
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    superscript: bool,
    subscript: bool,
    monospace: bool,
}

fn collect_inlines(children: &[HtmlNode], para: &mut Paragraph, bold: bool, italic: bool) {
    let style = InlineStyle {
        bold,
        italic,
        ..Default::default()
    };
    collect_inlines_styled(children, para, style);
}

fn collect_inlines_styled(children: &[HtmlNode], para: &mut Paragraph, style: InlineStyle) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = style.bold;
                    run.italic = style.italic;
                    run.superscript = style.superscript;
                    run.subscript = style.subscript;
                    run.monospace = style.monospace;
                    para.push_run(run);
                }
            }
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                if tag == "a" && has_attr_value(elem, "role", "doc-noteref") {
                    // Skip footnote reference links
                } else {
                    let new_style = InlineStyle {
                        bold: style.bold || tag == "strong" || tag == "b",
                        italic: style.italic || tag == "em" || tag == "i",
                        superscript: style.superscript || tag == "sup",
                        subscript: style.subscript || tag == "sub",
                        monospace: style.monospace || tag == "code",
                    };
                    collect_inlines_styled(&elem.children, para, new_style);
                }
            }
            HtmlNode::Tag(tag) => {
                if is_tag_start(tag, "footnote") {
                    // Get the location so we can find the matching End tag
                    let start_loc = tag.location();
                    // Find the footnote number by looking at the next children.
                    let footnote_id = find_footnote_id_in_range(&children[i..]);
                    if let Some(id) = footnote_id {
                        // Offset by +1 because OOXML reserves ids 0,1 for separators
                        para.add_footnote_ref(id + 1);
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
            HtmlNode::Frame(_) => {
                // Frame elements may contain images or other visual content.
                // For now, emit a placeholder text.
                let mut run = Run::new("[Image]");
                run.italic = true;
                para.push_run(run);
            }
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
    get_attr_value(elem, attr_name).as_deref() == Some(attr_value)
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
///
/// Also extracts document style (fonts, sizes) and page settings from the rendered output.
fn recover_missing_content(world: &TyportWorld, doc: &mut Document) {
    let paged_result = typst::compile::<PagedDocument>(world);
    let Ok(paged) = paged_result.output else {
        return;
    };

    // Extract document style and page settings from the PagedDocument
    doc.style = extract_document_style(&paged);
    extract_page_settings(&paged, &mut doc.page_settings);

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
    let full_doc_text = extract_doc_text(doc);

    let mut missing = Vec::new();
    for (i, line) in first_page_lines.iter().enumerate() {
        if i < title_line_count || i >= body_start_line {
            continue;
        }
        if line.text.chars().count() < 2 {
            continue;
        }
        if !title_section_text.contains(&line.text) && !full_doc_text.contains(&line.text) {
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
        let search_prefix: String = body_text.chars().take(10).collect();
        for (i, line) in paged_lines.iter().enumerate() {
            if line.text.contains(&search_prefix) {
                return i;
            }
        }
    }
    paged_lines.len()
}

/// A text line extracted from a `PagedDocument` frame, with its Y position (for ordering).
#[derive(Debug, Clone)]
/// A recovered line with text runs that preserve superscript info.
struct FrameLine {
    text: String,
    runs: Vec<Run>,
}

/// Extract text lines from the first page of a `PagedDocument`.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn extract_lines_from_first_page(paged: &PagedDocument) -> Vec<FrameLine> {
    let mut all_lines = Vec::new();

    let body_size = paged.pages.first().map_or(10.5, |p| {
        let mut items = Vec::new();
        collect_text_items_with_pos(&p.frame, Point::zero(), &mut items);
        let mut sizes: HashMap<i32, usize> = HashMap::new();
        for item in &items {
            *sizes.entry((item.size_pt * 10.0) as i32).or_default() += item.text.len();
        }
        sizes
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map_or(10.5, |(s, _)| f64::from(s) / 10.0)
    });

    for page in paged.pages.iter().take(1) {
        let mut text_items = Vec::new();
        collect_text_items_with_pos(&page.frame, Point::zero(), &mut text_items);

        let mut y_groups: BTreeMap<i64, Vec<&FrameTextItem>> = BTreeMap::new();
        for item in &text_items {
            // Use 8pt tolerance so superscript text (offset ~3-5pt up) stays grouped
            // with its base text on the same line
            let y_key = (item.y / 8.0).round() as i64;
            y_groups.entry(y_key).or_default().push(item);
        }

        for (_, mut items) in y_groups {
            items.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            let mut runs = Vec::new();
            let mut full_text = String::new();
            for item in &items {
                let is_super = item.size_pt < body_size * 0.8;
                let mut run = Run::new(&item.text);
                run.superscript = is_super;
                runs.push(run);
                full_text.push_str(&item.text);
            }
            let trimmed = full_text.trim().to_string();
            if !trimmed.is_empty() {
                all_lines.push(FrameLine {
                    text: trimmed,
                    runs,
                });
            }
        }
    }
    all_lines
}

/// A text fragment from a rendered frame with position and size info.
struct FrameTextItem {
    y: f64,
    x: f64,
    text: String,
    size_pt: f64,
}

/// Recursively collect text items from a frame with their absolute positions.
fn collect_text_items_with_pos(frame: &Frame, offset: Point, items: &mut Vec<FrameTextItem>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if !text.is_empty() {
                    items.push(FrameTextItem {
                        y: abs_y.to_pt(),
                        x: abs_x.to_pt(),
                        text,
                        size_pt: text_item.size.to_pt(),
                    });
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
    let insert_idx = find_title_section_end(doc);

    let mut paragraphs: Vec<BlockElement> = Vec::new();
    for line in missing_lines {
        let mut para = Paragraph::new();
        para.alignment = Some(Alignment::Center);
        para.suppress_indent = true;
        for run in &line.runs {
            para.push_run(run.clone());
        }
        paragraphs.push(BlockElement::Paragraph(para));
    }

    // Insert at the computed position
    if !paragraphs.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(paragraphs);
        doc.body.elements.extend(tail);
    }
}

/// Extract document style (fonts, sizes, spacing) from the rendered `PagedDocument`.
///
/// Walks all pages' frames to find the most common font family and size,
/// which represent the body text styling.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
    let mut font_counts: HashMap<String, usize> = HashMap::new();
    let mut size_counts: HashMap<u32, usize> = HashMap::new();

    // Walk all pages (but mainly first few to get representative body text)
    for page in paged.pages.iter().take(3) {
        collect_font_info(&page.frame, &mut font_counts, &mut size_counts);
    }

    // Most common font = body font
    let body_font = font_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or_else(|| "Times New Roman".to_string(), |(family, _)| family);

    // Most common size = body size (in half-points)
    let body_size_half_pt = size_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(21, |(size, _)| size); // default 10.5pt

    // Determine if the font looks like an East-Asian font or a Latin font.
    // Use same font for both ascii and eastAsia since Typst uses a single font stack.
    let body_font_ascii = body_font.clone();
    let body_font_east_asia = body_font;

    // Compute first-line indent: 2 chars at body size (standard CJK convention)
    // body_size_half_pt / 2 = pt; pt * 20 = twips; * 2 chars = indent
    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = (body_pt * 20.0 * 2.0).round() as u32;

    // Line spacing: default to 1.5x (360/240 of a line) — matching typical Typst defaults
    let line_spacing = 360;

    DocumentStyle {
        body_font_ascii,
        body_font_east_asia,
        body_size_half_pt,
        line_spacing,
        first_line_indent_twips,
    }
}

/// Recursively collect font family and size information from frame items.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_font_info(
    frame: &Frame,
    font_counts: &mut HashMap<String, usize>,
    size_counts: &mut HashMap<u32, usize>,
) {
    for (_pos, item) in frame.items() {
        match item {
            FrameItem::Text(text_item) => {
                let family = text_item.font.info().family.clone();
                let size_half_pt = (text_item.size.to_pt() * 2.0).round() as u32;
                *font_counts.entry(family).or_insert(0) += text_item.glyphs.len();
                *size_counts.entry(size_half_pt).or_insert(0) += text_item.glyphs.len();
            }
            FrameItem::Group(group) => {
                collect_font_info(&group.frame, font_counts, size_counts);
            }
            _ => {}
        }
    }
}

/// Extract page dimensions from the `PagedDocument` and apply to `PageSettings`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_page_settings(
    paged: &PagedDocument,
    settings: &mut typort_ooxml::document::PageSettings,
) {
    let Some(page) = paged.pages.first() else {
        return;
    };

    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    settings.width_twips = (page_width * 20.0).round() as u32;
    settings.height_twips = (page_height * 20.0).round() as u32;

    // Estimate margins from the content bounding box.
    // Walk frame items to find the extremes of content positioning.
    let mut min_x = page_width;
    let mut max_x: f64 = 0.0;
    let mut min_y = page_height;
    let mut max_y: f64 = 0.0;

    collect_content_bounds(
        &page.frame,
        Point::zero(),
        &mut min_x,
        &mut max_x,
        &mut min_y,
        &mut max_y,
    );

    if min_x < max_x && min_y < max_y {
        // Convert to twips, with a small tolerance (don't let margins be negative)
        let margin_left = (min_x * 20.0).round().max(0.0) as u32;
        let margin_right = ((page_width - max_x) * 20.0).round().max(0.0) as u32;
        let margin_top = (min_y * 20.0).round().max(0.0) as u32;
        let margin_bottom = ((page_height - max_y) * 20.0).round().max(0.0) as u32;

        // Only override if the calculated margins are reasonable (at least 100 twips ~ 0.5cm)
        if margin_left >= 100 {
            settings.margin_left = margin_left;
        }
        if margin_right >= 100 {
            settings.margin_right = margin_right;
        }
        if margin_top >= 100 {
            settings.margin_top = margin_top;
        }
        if margin_bottom >= 100 {
            settings.margin_bottom = margin_bottom;
        }
    }
}

/// Recursively collect content bounding box from frame items.
fn collect_content_bounds(
    frame: &Frame,
    offset: Point,
    min_x: &mut f64,
    max_x: &mut f64,
    min_y: &mut f64,
    max_y: &mut f64,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let x = abs_x.to_pt();
                let y = abs_y.to_pt();
                let w = text_item.width().to_pt();
                if x < *min_x {
                    *min_x = x;
                }
                if x + w > *max_x {
                    *max_x = x + w;
                }
                if y < *min_y {
                    *min_y = y;
                }
                // Approximate text height as the font size
                let h = text_item.size.to_pt();
                if y + h > *max_y {
                    *max_y = y + h;
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_content_bounds(&group.frame, new_offset, min_x, max_x, min_y, max_y);
            }
            _ => {}
        }
    }
}
