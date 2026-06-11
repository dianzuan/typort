use typort_ooxml::document::{Document, FootnoteFormat, InlineElement, Run};
use typst::introspection::Tag;
use typst::layout::PagedDocument;
use typst_html::HtmlNode;

use super::{get_text_content, has_attr_value, tag_name};

/// Extract the footnote bodies from the HTML `doc-endnotes` section, add them to the
/// document, and refine the footnote text size from the Paged render. The size is
/// measured from the footnote runs themselves (`page::apply_footnote_text_size`)
/// because the global size histogram mistakes the superscript reference marker for
/// the footnote body text.
pub(super) fn extract_add_and_size_footnotes(
    doc: &mut Document,
    body_children: &[HtmlNode],
    paged: Option<&PagedDocument>,
) {
    let contents = extract_footnote_contents(body_children);
    for content in &contents {
        doc.add_footnote(content.clone());
    }
    super::page::apply_footnote_text_size(doc, paged, &contents);
}

/// Find the footnote number from children starting at a TAG Start("footnote").
/// Looks for `<a role="doc-noteref">` -> `<sup>` -> text number.
pub(super) fn find_footnote_id_in_range(children: &[HtmlNode]) -> Option<u32> {
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

pub(super) fn parse_circled_number(s: &str) -> Option<u32> {
    let c = s.chars().next()?;
    let code = c as u32;
    match code {
        0x2460..=0x2473 => Some(code - 0x2460 + 1),
        0x3251..=0x325F => Some(code - 0x3251 + 21),
        0x32B1..=0x32BF => Some(code - 0x32B1 + 36),
        _ => None,
    }
}

/// Extract footnote content from the `<section role="doc-endnotes">` at the end of body.
pub(super) fn extract_footnote_contents(children: &[HtmlNode]) -> Vec<Vec<InlineElement>> {
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

fn extract_footnotes_from_ol(children: &[HtmlNode], footnotes: &mut Vec<Vec<InlineElement>>) {
    for child in children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut inlines = Vec::new();
            collect_footnote_inlines(&li.children, &mut inlines, false, false, false);
            footnotes.push(inlines);
        }
    }
}

/// Collect inline elements from footnote content, preserving formatting
/// and including math equations. Skips backlink anchors.
pub(super) fn collect_footnote_inlines(
    children: &[HtmlNode],
    inlines: &mut Vec<InlineElement>,
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
                    inlines.push(InlineElement::Text(run));
                }
            }
            HtmlNode::Element(elem) => {
                if has_attr_value(elem, "role", "doc-backlink") {
                    continue;
                }
                let tag = tag_name(elem);
                let new_bold = bold || tag == "strong" || tag == "b";
                let new_italic = italic || tag == "em" || tag == "i";
                let new_monospace = monospace || tag == "code";
                collect_footnote_inlines(
                    &elem.children,
                    inlines,
                    new_bold,
                    new_italic,
                    new_monospace,
                );
            }
            HtmlNode::Tag(Tag::Start(content, _)) => {
                if content.elem().name() == "equation" {
                    let omml = typort_math::equation_to_omml(content);
                    inlines.push(InlineElement::Math {
                        omml_xml: omml,
                        equation_number: None,
                    });
                }
            }
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
}

/// Detect whether footnotes use circled number format (①②③).
pub(super) fn detect_footnote_format(children: &[HtmlNode], doc: &mut Document) {
    for child in children {
        if let HtmlNode::Element(elem) = child {
            if has_attr_value(elem, "role", "doc-noteref")
                && let Some(sup) = elem
                    .children
                    .iter()
                    .find(|c| matches!(c, HtmlNode::Element(e) if tag_name(e) == "sup"))
                && let HtmlNode::Element(sup) = sup
                && let Some(text) = get_text_content(&sup.children)
            {
                let trimmed = text.trim();
                if parse_circled_number(trimmed).is_some() {
                    doc.style.footnote_format = FootnoteFormat::CircledNumber;
                    return;
                }
            }
            detect_footnote_format(&elem.children, doc);
            if doc.style.footnote_format == FootnoteFormat::CircledNumber {
                return;
            }
        }
    }
}
