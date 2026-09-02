use super::{
    Document, HtmlDocument, HtmlElement, HtmlNode, InlineFmt, InlineOptions, ListInfo, Location,
    Paragraph, WalkCtx, collect_inlines, find_first_element, find_tag_end, get_attr_value,
    tag_name, walk_tags,
};

pub(super) fn handle_list_tag(
    children: &[HtmlNode],
    i: usize,
    location: Location,
    ordered: bool,
    ctx: &mut WalkCtx,
) -> usize {
    let end = find_tag_end(children, i, location);
    handle_list(&children[i..=end], ordered, ctx);
    end
}

/// Handle a `list` or `enum` Tag: find the HTML `<ul>` or `<ol>` element in the inner
/// children and parse it.
pub(super) fn handle_list(slice: &[HtmlNode], ordered: bool, ctx: &mut WalkCtx) {
    let list_tag = if ordered { "ol" } else { "ul" };
    if let Some(list) = find_first_element(slice, &|element| tag_name(element) == list_tag) {
        convert_html_list(list, ctx.doc, ordered, ctx.html_doc);
        return;
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
}

/// Convert an HTML `<ol>` or `<ul>` element into list paragraphs.
pub(super) fn convert_html_list(
    elem: &HtmlElement,
    doc: &mut Document,
    ordered: bool,
    html_doc: &HtmlDocument,
) {
    // typst-html carries `#enum(start: N)` as `<ol start="N">`; Word needs it
    // back as the numbering instance's level-0 startOverride.
    let start = get_attr_value(elem, "start")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let list_id = doc.allocate_list_id(ordered, start);
    convert_html_list_at_level(elem, doc, 0, list_id, html_doc);
}

pub(super) fn convert_html_list_at_level(
    elem: &HtmlElement,
    doc: &mut Document,
    level: u32,
    list_id: u32,
    html_doc: &HtmlDocument,
) {
    let is_sublist = |c: &HtmlNode| {
        matches!(c, HtmlNode::Element(el) if {
            let t = tag_name(el);
            t == "ul" || t == "ol"
        })
    };
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            para.list_info = Some(ListInfo { id: list_id, level });
            // Route the item's direct inline content (everything but nested
            // sub-lists) through the standard inline collector, so equation
            // Tags produce OMML and their sibling MathML `<math>` elements are
            // skipped — the bespoke per-child loop this replaces descended
            // into `<math>` and leaked its glyphs as literal text. Contiguous
            // ranges (not per-node slices) preserve the sibling context the
            // footnote-id lookup needs.
            let mut range_start = 0;
            for idx in 0..=li.children.len() {
                if idx == li.children.len() || is_sublist(&li.children[idx]) {
                    if range_start < idx {
                        collect_inlines(
                            &li.children[range_start..idx],
                            &mut para,
                            None,
                            InlineOptions::generic(InlineFmt::default(), Some(html_doc)),
                        );
                    }
                    range_start = idx + 1;
                }
            }
            if !para.inlines.is_empty() {
                doc.add_paragraph(para);
            }
            for li_child in &li.children {
                if let HtmlNode::Element(sub) = li_child
                    && is_sublist(li_child)
                {
                    convert_html_list_at_level(sub, doc, level + 1, list_id, html_doc);
                }
            }
        }
    }
}
