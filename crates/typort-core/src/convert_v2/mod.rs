//! Tag-walker based Typst -> OOXML conversion (v2).
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

pub mod inline;
pub mod page;

use typort_ooxml::document::{
    BlockElement, Document, Paragraph, ParagraphStyle, Run, Table, TableCell, TableRow, VMerge,
};
use typst::foundations::StyleChain;
use typst::introspection::Tag;
use typst::layout::PagedDocument;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;
use typst_library::model::HeadingElem;

use crate::world::TyportWorld;

/// Convert a Typst source file to an OOXML `Document` using the tag-walker approach.
///
/// # Errors
/// Returns compilation errors if the Typst source cannot be compiled.
pub fn convert(world: &TyportWorld) -> Result<Document, Vec<String>> {
    // 1. Compile to HtmlDocument for semantic structure + introspector
    let html_result = typst::compile::<HtmlDocument>(world);
    let html_doc = match html_result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };

    // 2. Compile to PagedDocument for page settings + font detection
    let paged_result = typst::compile::<PagedDocument>(world);
    let paged_doc = paged_result.output.ok();

    let mut doc = Document::new();

    // 3. Extract page settings and document style from PagedDocument
    if let Some(paged) = &paged_doc {
        doc.style = page::extract_document_style(paged);
        page::extract_page_settings(paged, &mut doc.page_settings);
    }

    // 4. First pass: extract footnote content from <section role="doc-endnotes">
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);
    let footnote_contents = extract_footnote_contents(&body.children);
    for content in &footnote_contents {
        doc.add_footnote(content.clone());
    }

    // 5. Walk the HTML tree's Tag sequence
    walk_tags(&body.children, &html_doc, &mut doc);

    // 6. Extract title from first heading
    extract_title_from_first_heading(&mut doc);

    Ok(doc)
}

/// Recursively walk `HtmlNode` children, dispatching on `Tag::Start` element types.
fn walk_tags(children: &[HtmlNode], html_doc: &HtmlDocument, doc: &mut Document) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    let elem_name = content.elem().name();
                    match elem_name {
                        "heading" => handle_heading(tag, html_doc, doc),
                        "par" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_par(&children[i..=end], html_doc, doc);
                            i = end;
                        }
                        "equation" => {
                            handle_equation(tag, html_doc, doc);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "footnote" => {
                            handle_block_footnote(tag, &children[i..], html_doc, doc);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "table" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_table(&children[i..=end], html_doc, doc);
                            i = end;
                        }
                        "list" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(&children[i..=end], html_doc, doc, false);
                            i = end;
                        }
                        "enum" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(&children[i..=end], html_doc, doc, true);
                            i = end;
                        }
                        "figure" | "section" => {
                            // Recurse into inner children between Start and End
                            let end = find_tag_end(children, i, tag.location());
                            // Skip doc-endnotes sections
                            if elem_name == "section"
                                && is_doc_endnotes_section(&children[i..=end])
                            {
                                i = end;
                                i += 1;
                                continue;
                            }
                            walk_tags(&children[i + 1..end], html_doc, doc);
                            i = end;
                        }
                        // Inline elements handled within par/collect_par_inlines,
                        // or should be skipped at block level. Also skip unknown tags.
                        _ => {}
                    }
                }
                // Tag::End is consumed implicitly
            }
            HtmlNode::Element(elem) => {
                handle_html_element(elem, html_doc, doc);
            }
            HtmlNode::Text(text, _) => {
                // Bare text outside of any Tag — emit as a paragraph
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.add_run(trimmed);
                    doc.add_paragraph(para);
                }
            }
            HtmlNode::Frame(_) => {
                // Frame nodes are layout artifacts; skip in tag walker.
            }
        }
        i += 1;
    }
}

/// Handle an HTML element (non-Tag node) — dispatches on element tag name.
fn handle_html_element(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
    doc: &mut Document,
) {
    let tag = tag_name(elem);
    match tag.as_str() {
        "pre" => convert_code_block(elem, doc),
        "blockquote" => convert_blockquote(elem, html_doc, doc),
        "dl" => convert_term_list(elem, doc),
        "ol" => convert_html_list(elem, doc, true),
        "ul" => convert_html_list(elem, doc, false),
        "table" => convert_html_table(elem, doc),
        "section" => {
            // Skip doc-endnotes section
            if has_attr_value(elem, "role", "doc-endnotes") {
                return;
            }
            walk_tags(&elem.children, html_doc, doc);
        }
        _ => {
            // Recurse into other HTML elements
            walk_tags(&elem.children, html_doc, doc);
        }
    }
}

/// Handle a `HeadingElem` tag: query the introspector for the full Content,
/// extract level + body runs, and emit a heading paragraph.
fn handle_heading(tag: &Tag, html_doc: &HtmlDocument, doc: &mut Document) {
    let loc = tag.location();
    let Some(content) = html_doc
        .introspector
        .query_first(&typst::foundations::Selector::Location(loc))
    else {
        return;
    };

    let Some(heading) = content.to_packed::<HeadingElem>() else {
        return;
    };

    let level = heading.resolve_level(StyleChain::default()).get();
    #[allow(clippy::cast_possible_truncation)]
    let level_u8 = level.min(255) as u8;

    let runs = inline::extract_runs(&heading.body);

    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level_u8));
    for run in runs {
        para.push_run(run);
    }
    doc.add_paragraph(para);
}

/// Handle a `par` Tag: collect inline children (text, strong, emph, equation, footnote)
/// and emit a paragraph.
fn handle_par(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
) {
    let mut para = Paragraph::new();
    // Skip the first Tag::Start("par") and collect inlines from the inner nodes
    let inner = &slice[1..slice.len().saturating_sub(1)];
    collect_par_inlines(inner, html_doc, doc, &mut para);
    if !para.runs.is_empty() || !para.inlines.is_empty() {
        doc.add_paragraph(para);
    }
}

/// Collect inline elements from nodes inside a paragraph.
/// This handles Text, `Tag::Start` for strong/emph/equation/footnote, and HTML elements.
fn collect_par_inlines(
    children: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    para.push_run(Run::new(text.as_str()));
                }
            }
            HtmlNode::Tag(tag) => {
                if let Tag::Start(..) = tag {
                    i = handle_inline_tag(tag, children, i, html_doc, doc, para);
                }
            }
            HtmlNode::Element(elem) => {
                handle_inline_html_element(elem, html_doc, doc, para);
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

/// Process a single inline `Tag::Start` within a paragraph.
/// Returns the new index (pointing at the matching End tag).
fn handle_inline_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
) -> usize {
    let Tag::Start(content, _) = tag else {
        return i;
    };
    let elem_name = content.elem().name();
    match elem_name {
        "strong" => {
            let loc = tag.location();
            if let Some(strong) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                .and_then(|c| c.to_packed::<typst_library::model::StrongElem>().cloned())
            {
                for mut r in inline::extract_runs(&strong.body) {
                    r.bold = true;
                    para.push_run(r);
                }
            }
            find_tag_end(children, i, tag.location())
        }
        "emph" => {
            let loc = tag.location();
            if let Some(emph) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                .and_then(|c| c.to_packed::<typst_library::model::EmphElem>().cloned())
            {
                for mut r in inline::extract_runs(&emph.body) {
                    r.italic = true;
                    para.push_run(r);
                }
            }
            find_tag_end(children, i, tag.location())
        }
        "equation" => {
            let loc = tag.location();
            if let Some(c) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
            {
                let omml = typort_math::equation_to_omml(&c);
                let is_block = c
                    .to_packed::<EquationElem>()
                    .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));
                if is_block {
                    if !para.runs.is_empty() || !para.inlines.is_empty() {
                        let prev = std::mem::take(para);
                        doc.add_paragraph(prev);
                    }
                    let mut math_para = Paragraph::new();
                    math_para.add_math(omml);
                    doc.add_paragraph(math_para);
                } else {
                    para.add_math(omml);
                }
            }
            find_tag_end(children, i, tag.location())
        }
        "footnote" => {
            let start_loc = tag.location();
            if let Some(id) = find_footnote_id_in_range(&children[i..]) {
                para.add_footnote_ref(id + 1);
            }
            find_tag_end(children, i, start_loc)
        }
        _ => {
            // Skip unknown or non-inline tags
            find_tag_end(children, i, tag.location())
        }
    }
}

/// Process a single inline HTML element within a paragraph.
fn handle_inline_html_element(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
) {
    let tag_str = tag_name(elem);
    match tag_str.as_str() {
        "strong" | "b" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, true, false, false);
            for run in tmp.runs {
                para.push_run(run);
            }
        }
        "em" | "i" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, true, false);
            for run in tmp.runs {
                para.push_run(run);
            }
        }
        "code" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, false, true);
            for run in tmp.runs {
                para.push_run(run);
            }
        }
        "a" if has_attr_value(elem, "role", "doc-noteref") => {
            // Already handled by Tag::Start("footnote")
        }
        "sup" | "sub" => {
            // Skip, consumed by footnote
        }
        _ => {
            collect_par_inlines(&elem.children, html_doc, doc, para);
        }
    }
}

/// Collect inline elements from HTML nodes (used for table cells, list items, etc.)
fn collect_html_inlines(
    children: &[HtmlNode],
    para: &mut Paragraph,
    bold: bool,
    italic: bool,
    monospace: bool,
) {
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    run.italic = italic;
                    run.monospace = monospace;
                    para.push_run(run);
                }
            }
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                let new_bold = bold || tag == "strong" || tag == "b";
                let new_italic = italic || tag == "em" || tag == "i";
                let new_monospace = monospace || tag == "code";
                // Skip footnote reference links
                if tag == "a" && has_attr_value(elem, "role", "doc-noteref") {
                    continue;
                }
                collect_html_inlines(&elem.children, para, new_bold, new_italic, new_monospace);
            }
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
}

/// Handle a block-level equation Tag.
fn handle_equation(tag: &Tag, html_doc: &HtmlDocument, doc: &mut Document) {
    let loc = tag.location();
    let Some(content) = html_doc
        .introspector
        .query_first(&typst::foundations::Selector::Location(loc))
    else {
        return;
    };

    let omml = typort_math::equation_to_omml(&content);
    let eq_packed = content.to_packed::<EquationElem>();
    let is_block = eq_packed
        .as_ref()
        .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));

    let mut para = Paragraph::new();
    if is_block {
        para.add_math(omml);
        doc.add_paragraph(para);
    } else {
        // Inline equation at block level: wrap in a paragraph
        para.add_math(omml);
        doc.add_paragraph(para);
    }
}

/// Handle a block-level footnote Tag.
fn handle_block_footnote(
    tag: &Tag,
    children_from_here: &[HtmlNode],
    _html_doc: &HtmlDocument,
    doc: &mut Document,
) {
    let footnote_id = find_footnote_id_in_range(children_from_here);
    if let Some(id) = footnote_id {
        // Add footnote ref to the last paragraph in the document
        if let Some(BlockElement::Paragraph(para)) = doc.body.elements.last_mut() {
            para.add_footnote_ref(id + 1);
        } else {
            // Create a new paragraph for the footnote ref
            let mut para = Paragraph::new();
            para.add_footnote_ref(id + 1);
            doc.add_paragraph(para);
        }
    }
    let _ = tag;
}

/// Handle a `table` Tag: find the HTML `<table>` element in the inner children and parse it.
fn handle_table(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
) {
    // Look for an HTML <table> element within the tag range
    for node in slice {
        if let HtmlNode::Element(elem) = node {
            let tag = tag_name(elem);
            if tag == "table" {
                convert_html_table(elem, doc);
                return;
            }
            // Recurse into child elements to find the table
            if find_and_convert_table_in_elem(elem, doc) {
                return;
            }
        }
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, html_doc, doc);
}

/// Recursively search for a `<table>` element within an HTML element tree.
fn find_and_convert_table_in_elem(elem: &HtmlElement, doc: &mut Document) -> bool {
    for child in &elem.children {
        if let HtmlNode::Element(inner) = child {
            let tag = tag_name(inner);
            if tag == "table" {
                convert_html_table(inner, doc);
                return true;
            }
            if find_and_convert_table_in_elem(inner, doc) {
                return true;
            }
        }
    }
    false
}

/// Handle a `list` or `enum` Tag: find the HTML `<ul>` or `<ol>` element in the inner
/// children and parse it.
fn handle_list(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    ordered: bool,
) {
    // Look for an HTML <ul> or <ol> element within the tag range
    for node in slice {
        if let HtmlNode::Element(elem) = node {
            let tag = tag_name(elem);
            if (ordered && tag == "ol") || (!ordered && tag == "ul") {
                convert_html_list(elem, doc, ordered);
                return;
            }
            // Recurse
            if find_and_convert_list_in_elem(elem, doc, ordered) {
                return;
            }
        }
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, html_doc, doc);
}

/// Recursively search for a `<ul>` or `<ol>` element.
fn find_and_convert_list_in_elem(
    elem: &HtmlElement,
    doc: &mut Document,
    ordered: bool,
) -> bool {
    for child in &elem.children {
        if let HtmlNode::Element(inner) = child {
            let tag = tag_name(inner);
            if (ordered && tag == "ol") || (!ordered && tag == "ul") {
                convert_html_list(inner, doc, ordered);
                return true;
            }
            if find_and_convert_list_in_elem(inner, doc, ordered) {
                return true;
            }
        }
    }
    false
}

/// Convert an HTML `<table>` element into the document model.
fn convert_html_table(elem: &HtmlElement, doc: &mut Document) {
    let mut table = Table { rows: Vec::new() };
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                if let Some(row) = convert_table_row(row_or_section) {
                    table.rows.push(row);
                }
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                        && let Some(row) = convert_table_row(tr)
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

/// Convert a `<tr>` element into a `TableRow`.
fn convert_table_row(tr: &HtmlElement) -> Option<TableRow> {
    let mut cells = Vec::new();
    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                let mut para = Paragraph::new();
                collect_html_inlines(&td.children, &mut para, tag == "th", false, false);

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

/// Convert an HTML `<ol>` or `<ul>` element into list paragraphs.
fn convert_html_list(elem: &HtmlElement, doc: &mut Document, ordered: bool) {
    let list_id = if ordered { 1 } else { 2 };
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            para.list_id = Some(list_id);
            para.list_level = Some(0);
            collect_html_inlines(&li.children, &mut para, false, false, false);
            if !para.runs.is_empty() {
                doc.add_paragraph(para);
            }
        }
    }
}

/// Convert a `<pre>` code block into monospace paragraphs (one per line).
fn convert_code_block(elem: &HtmlElement, doc: &mut Document) {
    let text = collect_all_text(&elem.children);
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
    html_doc: &HtmlDocument,
    doc: &mut Document,
) {
    let start_idx = doc.body.elements.len();
    walk_tags(&elem.children, html_doc, doc);
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
                    collect_html_inlines(&item.children, &mut para, true, false, false);
                    if !para.runs.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                "dd" => {
                    let mut para = Paragraph::new();
                    para.left_indent = Some(420);
                    para.suppress_indent = true;
                    collect_html_inlines(&item.children, &mut para, false, false, false);
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

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Find the index of the `Tag::End` matching the given start location.
fn find_tag_end(children: &[HtmlNode], start_idx: usize, start_loc: typst::introspection::Location) -> usize {
    let mut j = start_idx + 1;
    while j < children.len() {
        if let HtmlNode::Tag(end_tag) = &children[j]
            && is_tag_end_for(end_tag, start_loc)
        {
            return j;
        }
        j += 1;
    }
    // If no matching end found, return the last index
    children.len().saturating_sub(1)
}

/// Check if a section contains a doc-endnotes section element.
fn is_doc_endnotes_section(slice: &[HtmlNode]) -> bool {
    for node in slice {
        if let HtmlNode::Element(elem) = node
            && has_attr_value(elem, "role", "doc-endnotes")
        {
            return true;
        }
    }
    false
}

/// Locate the `<body>` element in the HTML tree.
fn find_body(root: &HtmlElement) -> Option<&HtmlElement> {
    for child in &root.children {
        if let HtmlNode::Element(elem) = child {
            let tag = format!("{}", elem.tag);
            if tag.contains("body") {
                return Some(elem);
            }
            if let Some(found) = find_body(elem) {
                return Some(found);
            }
        }
    }
    None
}

/// Set the document title from the first heading's text.
fn extract_title_from_first_heading(doc: &mut Document) {
    for elem in &doc.body.elements {
        if let BlockElement::Paragraph(p) = elem
            && matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            let title: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            if !title.is_empty() {
                doc.metadata.title = Some(title);
            }
            return;
        }
    }
}

/// Get the tag name of an HTML element.
fn tag_name(elem: &HtmlElement) -> String {
    let raw = format!("{}", elem.tag);
    raw.trim_matches('<').trim_matches('>').to_string()
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

/// Find the footnote number from the children starting at a TAG Start("footnote").
/// Looks for <a role="doc-noteref"> -> <sup> -> text number.
fn find_footnote_id_in_range(children: &[HtmlNode]) -> Option<u32> {
    for child in children {
        match child {
            HtmlNode::Element(elem) => {
                if has_attr_value(elem, "role", "doc-noteref") {
                    return find_sup_number(&elem.children);
                }
                if let Some(id) = find_footnote_id_in_range(&elem.children) {
                    return Some(id);
                }
            }
            HtmlNode::Tag(tag) => {
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
                let trimmed = text.trim();
                if let Ok(n) = trimmed.parse() {
                    return Some(n);
                }
                if let Some(n) = parse_circled_number(trimmed) {
                    return Some(n);
                }
            }
            if let Some(n) = find_sup_number(&elem.children) {
                return Some(n);
            }
        }
    }
    None
}

/// Parse a circled number character into its numeric value.
fn parse_circled_number(s: &str) -> Option<u32> {
    let c = s.chars().next()?;
    let code = c as u32;
    match code {
        0x2460..=0x2473 => Some(code - 0x2460 + 1),
        0x3251..=0x325F => Some(code - 0x3251 + 21),
        0x32B1..=0x32BF => Some(code - 0x32B1 + 36),
        _ => None,
    }
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
fn extract_footnote_contents(children: &[HtmlNode]) -> Vec<Vec<Run>> {
    let mut footnotes = Vec::new();

    for child in children {
        if let HtmlNode::Element(elem) = child
            && tag_name(elem) == "section"
            && has_attr_value(elem, "role", "doc-endnotes")
        {
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
            let mut runs = Vec::new();
            collect_footnote_text(&li.children, &mut runs);
            footnotes.push(runs);
        }
    }
}

/// Collect text from footnote <li> content, skipping the backlink anchor.
fn collect_footnote_text(children: &[HtmlNode], runs: &mut Vec<Run>) {
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    runs.push(Run::new(text.as_str()));
                }
            }
            HtmlNode::Element(elem) => {
                if has_attr_value(elem, "role", "doc-backlink") {
                    continue;
                }
                collect_footnote_text(&elem.children, runs);
            }
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
}
