//! Horizontal-rule recovery and the element-to-page map used for placement.

use std::num::NonZeroUsize;

use typort_ooxml::document::{BlockElement, Document, Paragraph};
use typst::introspection::{Introspector, Location, Tag};
use typst::layout::{Frame, FrameItem};
use typst_html::HtmlNode;
use typst_layout::PagedDocument;

use super::super::dom::find_tag_end;
use super::deduplication::extract_doc_text;

/// Page-width fraction a line must span to be a document rule candidate.
const HORIZONTAL_RULE_PAGE_WIDTH_RATIO: f64 = 0.6;
/// Minimum document-text bytes required before inserting a recovered rule.
const MIN_RULE_DOCUMENT_TEXT_BYTES: usize = 10;
/// Maximum vertical extent in points for a horizontal document rule.
const MAX_HORIZONTAL_RULE_HEIGHT_PT: f64 = 5.0;

/// Build a mapping from document body element index to page number.
///
/// Recovered paragraphs carry their real page in `page_from_paged` and use it
/// directly. Remaining elements (the ones that came from HTML block tags) are
/// mapped from those tags' introspector pages; only as a last resort — when tag
/// count and element count disagree — is a proportional interpolation used.
pub(in crate::convert) fn build_element_page_map(
    doc: &Document,
    children: &[HtmlNode],
    paged: &PagedDocument,
) -> Vec<usize> {
    let total_elements = doc.body.elements.len();
    if total_elements == 0 || paged.pages().is_empty() {
        return Vec::new();
    }

    let mut locs: Vec<Location> = Vec::new();
    collect_block_tag_locations(children, &mut locs);

    if locs.is_empty() {
        // No tag positions at all: use recovered pages where known, else page 1.
        return doc
            .body
            .elements
            .iter()
            .map(|el| match el {
                BlockElement::Paragraph(p) => p.page_from_paged.unwrap_or(1),
                _ => 1,
            })
            .collect();
    }

    // `Introspector::page` now returns `Option<NonZeroUsize>` (0.14 returned the
    // `NonZeroUsize` directly); a location without a known page falls back to 1.
    let tag_pages: Vec<usize> = locs
        .iter()
        .map(|loc| paged.introspector().page(*loc).map_or(1, NonZeroUsize::get))
        .collect();

    let n_tags = tag_pages.len();
    let mut result = vec![1_usize; total_elements];
    // Walk tag-derived pages and recovered pages in parallel: elements with a
    // known recovered page take it; the rest consume the next tag page in order.
    let mut tag_cursor = 0;
    for (elem_idx, slot) in result.iter_mut().enumerate() {
        if let BlockElement::Paragraph(p) = &doc.body.elements[elem_idx]
            && let Some(page) = p.page_from_paged
        {
            *slot = page;
            continue;
        }
        let tag_idx = if n_tags >= total_elements {
            elem_idx.min(n_tags - 1)
        } else {
            tag_cursor.min(n_tags - 1)
        };
        *slot = tag_pages[tag_idx];
        tag_cursor += 1;
    }

    result
}

/// Whether the source AST contains a `#line(...)` call — the only construct that
/// should become a body horizontal rule. `#line()` has no HTML element and is
/// consumed during layout, so it is recovered from geometry; but a table's
/// border rules and a footnote separator are *also* wide horizontal lines in the
/// geometry. Gating rule recovery on a real `line()` call stops those from being
/// invented as body rules (the same source-AST-authority rule as colbreak).
fn source_declares_line_rule(source: &str) -> bool {
    fn has_line_call(node: &typst_syntax::SyntaxNode) -> bool {
        if node.kind() == typst_syntax::SyntaxKind::FuncCall
            && node
                .cast::<typst_syntax::ast::FuncCall<'_>>()
                .is_some_and(|fc| {
                    matches!(fc.callee(), typst_syntax::ast::Expr::Ident(i) if i.as_str() == "line")
                })
        {
            return true;
        }
        node.children().any(has_line_call)
    }
    has_line_call(&typst_syntax::parse(source))
}

/// Detect horizontal line shapes and insert horizontal rule paragraphs.
pub(in crate::convert) fn insert_horizontal_rules_from_paged(
    paged: &PagedDocument,
    doc: &mut Document,
    element_page_map: &[usize],
    sources: &[String],
) {
    // Only recover rules the source actually declares with `#line()`; without one,
    // a wide line in the geometry is a table border or a footnote separator. The
    // declaration may live in ANY reachable source file — a template imported by
    // the main file draws its separators just as authoritatively.
    if !sources.iter().any(|s| source_declares_line_rule(s)) {
        return;
    }
    let total_pages = paged.pages().len();
    if total_pages == 0 {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    let mut hrules: Vec<(usize, f64)> = Vec::new();

    for (page_idx, page) in paged.pages().iter().enumerate() {
        let page_width = page.frame.width().to_pt();
        let content_width = page_width * HORIZONTAL_RULE_PAGE_WIDTH_RATIO;

        let mut lines = Vec::new();
        collect_horizontal_lines(&page.frame, content_width, &mut lines);

        for line_y in lines {
            hrules.push((page_idx + 1, line_y));
        }
    }

    if hrules.is_empty() {
        return;
    }

    hrules.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let full_doc_text = extract_doc_text(doc);

    for (page_num, _line_y) in &hrules {
        let insert_idx = if !element_page_map.is_empty() && element_page_map.len() == total_elements
        {
            element_page_map
                .iter()
                .position(|&p| p >= *page_num)
                .unwrap_or(total_elements)
        } else {
            super::super::stats::proportional_index_rounded(
                page_num.saturating_sub(1),
                total_pages,
                total_elements,
            )
            .min(total_elements)
        };

        let already_has_hrule = doc.body.elements.get(insert_idx).is_some_and(|e| {
            if let BlockElement::Paragraph(p) = e {
                p.horizontal_rule
            } else {
                false
            }
        });

        if already_has_hrule {
            continue;
        }

        if full_doc_text.len() < MIN_RULE_DOCUMENT_TEXT_BYTES {
            continue;
        }

        let mut hr_para = Paragraph::new();
        hr_para.horizontal_rule = true;
        doc.body
            .elements
            .insert(insert_idx, BlockElement::Paragraph(hr_para));
    }
}

fn collect_horizontal_lines(frame: &Frame, min_width: f64, lines: &mut Vec<f64>) {
    super::super::frames::visit_frame_items(frame, true, &mut |position, item| {
        if let FrameItem::Shape(shape, _) = item
            && let typst::visualize::Geometry::Line(end_pt) = &shape.geometry
        {
            let line_width = end_pt.x.to_pt().abs();
            let line_height = end_pt.y.to_pt().abs();
            if line_width >= min_width && line_height < MAX_HORIZONTAL_RULE_HEIGHT_PT {
                lines.push(position.y.to_pt());
            }
        }
    });
}

/// Recursively collect `Location`s of block-level `Tag::Start` nodes from
/// the HTML tree, preserving document order.  Only introspectable tags for
/// block-level elements (heading, par, equation, table, list, enum, figure,
/// image, section, outline) are collected — these are the tags whose page
/// numbers are meaningful for page-break detection.
fn collect_block_tag_locations(children: &[HtmlNode], out: &mut Vec<Location>) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, flags) = tag
                    && flags.introspectable
                {
                    let name = content.elem().name();
                    match name {
                        "heading" | "par" | "equation" | "table" | "list" | "enum" | "figure"
                        | "image" | "outline" => {
                            out.push(tag.location());
                        }
                        "section" => {
                            // Recurse into sections (but record the section
                            // itself so that page boundaries at section
                            // starts are detected).
                            out.push(tag.location());
                            let end = find_tag_end(children, i, tag.location());
                            collect_block_tag_locations(&children[i + 1..end], out);
                            i = end + 1;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            HtmlNode::Element(elem) => {
                // Recurse into HTML elements (e.g. <div>, <section>)
                collect_block_tag_locations(&elem.children, out);
            }
            _ => {}
        }
        i += 1;
    }
}
