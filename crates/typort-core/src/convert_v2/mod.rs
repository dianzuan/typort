//! Tag-walker based Typst -> OOXML conversion (v2).
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

pub mod inline;
pub mod page;

use typort_ooxml::document::{
    BlockElement, Document, Paragraph, ParagraphStyle,
};
use typst::foundations::StyleChain;
use typst::introspection::Tag;
use typst::layout::PagedDocument;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
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

    // 4. Walk the HTML tree's Tag sequence
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);
    walk_tags(&body.children, &html_doc, &mut doc);

    // 5. Extract title from first heading
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
                    if elem_name == "heading" {
                        handle_heading(tag, html_doc, doc);
                    }
                    // Future: "equation", "footnote", "figure", "table", etc.
                }
                // Tag::End is consumed implicitly
            }
            HtmlNode::Element(elem) => {
                // Recurse into HTML element children
                walk_tags(&elem.children, html_doc, doc);
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
