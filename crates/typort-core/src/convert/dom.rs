use super::{
    Alignment, Content, EquationElem, HtmlDocument, HtmlElement, HtmlNode, InlineElement,
    Introspector, Location, NativeElement, Packed, Paragraph, Run, Selector, Tag,
};

/// Query the semantic content attached to an introspection location.
pub(super) fn content_at_location(html_doc: &HtmlDocument, location: Location) -> Option<Content> {
    html_doc
        .introspector()
        .query_first(&Selector::Location(location))
}

/// Query and downcast the semantic element attached to an introspection location.
pub(super) fn element_at_location<E: NativeElement>(
    html_doc: &HtmlDocument,
    location: Location,
) -> Option<Packed<E>> {
    content_at_location(html_doc, location).and_then(|content| content.into_packed::<E>().ok())
}

/// Construct a text run and retain a source span when one is attached.
pub(super) fn run_with_span(text: &str, span: typst_syntax::Span) -> Run {
    let mut run = Run::new(text);
    if !span.is_detached() {
        run.span = Some(span);
    }
    run
}

/// Whether an introspected equation is block-level.
pub(super) fn is_block_equation(content: &Content) -> bool {
    content
        .to_packed::<EquationElem>()
        .is_some_and(|equation| *equation.block.as_option().as_ref().unwrap_or(&false))
}
/// Find the first nested HTML element matching `predicate`, in document order.
pub(super) fn find_first_element<'a, F>(
    nodes: &'a [HtmlNode],
    predicate: &F,
) -> Option<&'a HtmlElement>
where
    F: Fn(&HtmlElement) -> bool,
{
    for node in nodes {
        if let HtmlNode::Element(element) = node {
            if predicate(element) {
                return Some(element);
            }
            if let Some(found) = find_first_element(&element.children, predicate) {
                return Some(found);
            }
        }
    }
    None
}

/// The `src` attribute of the first `<img>` element within a node range.
pub(super) fn find_img_src(children: &[HtmlNode]) -> Option<String> {
    find_first_element(children, &|element| tag_name(element) == "img")
        .and_then(|element| get_attr_value(element, "src"))
}

/// Whether every direct child of `nodes` is inline-level content (text, an inline
/// formatting span, an inline equation, …) rather than a block.
///
/// Used to decide whether a block container holds a single inline paragraph (which
/// must be collected as one paragraph so its text and inline equations stay
/// together) or genuine block children (which `walk_tags` should handle). A node is
/// treated as a block only if it is a known block-level HTML element or block Typst
/// tag; everything else (including bare text and `Tag::End`) counts as inline.
pub(super) fn children_are_inline(nodes: &[HtmlNode]) -> bool {
    const BLOCK_HTML: &[&str] = &[
        "p",
        "div",
        "section",
        "figure",
        "figcaption",
        "table",
        "ul",
        "ol",
        "li",
        "dl",
        "blockquote",
        "pre",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
    ];
    const BLOCK_TAGS: &[&str] = &[
        "heading",
        "par",
        "list",
        "enum",
        "table",
        "figure",
        "outline",
        "footnote",
        "block",
        "grid",
        "columns",
        "pagebreak",
        "list-item",
        "enum-item",
        "terms",
    ];
    nodes.iter().all(|node| match node {
        HtmlNode::Element(elem) => !BLOCK_HTML.contains(&tag_name(elem).as_str()),
        HtmlNode::Tag(Tag::Start(content, _)) => !BLOCK_TAGS.contains(&content.elem().name()),
        _ => true,
    })
}

/// Whether a figure subtree contains an element with the given Typst tag name.
/// Checks both flattened `Tag::Start` markers and nested `Element` children, so
/// it works regardless of which HTML representation the construct took.
pub(super) fn subtree_has_element(nodes: &[HtmlNode], name: &str) -> bool {
    nodes.iter().any(|node| match node {
        HtmlNode::Tag(Tag::Start(content, _)) => content.elem().name() == name,
        HtmlNode::Element(elem) => {
            tag_name(elem).as_str() == name || subtree_has_element(&elem.children, name)
        }
        _ => false,
    })
}

/// Recursively collect all text content from a node tree.
pub(super) fn collect_deep_text(children: &[HtmlNode]) -> String {
    let mut text = String::new();
    let mut line_started = false;
    for child in children {
        match child {
            HtmlNode::Text(t, _) => text.push_str(t),
            HtmlNode::Element(elem) => text.push_str(&collect_deep_text(&elem.children)),
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
/// Collect all text content from a slice of `HtmlNode` (used for cross-reference display text).
pub(super) fn collect_flat_text(nodes: &[HtmlNode]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(t, _) => text.push_str(t),
            HtmlNode::Element(elem) => text.push_str(&collect_deep_text(&elem.children)),
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
    text
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Find the index of the `Tag::End` matching the given start location.
pub(super) fn find_tag_end(
    children: &[HtmlNode],
    start_idx: usize,
    start_loc: typst::introspection::Location,
) -> usize {
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
pub(super) fn is_doc_endnotes_section(slice: &[HtmlNode]) -> bool {
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
pub(super) fn find_body(root: &HtmlElement) -> Option<&HtmlElement> {
    for child in &root.children {
        if let HtmlNode::Element(elem) = child {
            if elem.tag.resolve().as_str() == "body" {
                return Some(elem);
            }
            if let Some(found) = find_body(elem) {
                return Some(found);
            }
        }
    }
    None
}
/// Get the tag name of an HTML element.
pub(super) fn tag_name(elem: &HtmlElement) -> String {
    elem.tag.resolve().as_str().to_string()
}

/// Drain all `InlineElement::Text` runs from a paragraph, consuming them.
pub(super) fn drain_text_runs(para: &mut Paragraph) -> Vec<Run> {
    para.inlines
        .drain(..)
        .filter_map(|i| {
            if let InlineElement::Text(run) = i {
                Some(run)
            } else {
                None
            }
        })
        .collect()
}

/// Check if a `Tag` is a Start tag with the given element name.
pub(super) fn is_tag_start(tag: &Tag, name: &str) -> bool {
    if let Tag::Start(content, _) = tag {
        content.elem().name() == name
    } else {
        false
    }
}

/// Check if a `Tag` is the End tag matching a given start location.
pub(super) fn is_tag_end_for(tag: &Tag, start_loc: typst::introspection::Location) -> bool {
    if let Tag::End(loc, ..) = tag {
        *loc == start_loc
    } else {
        false
    }
}

/// Check if an element has a specific attribute value.
pub(super) fn has_attr_value(elem: &HtmlElement, attr_name: &str, attr_value: &str) -> bool {
    get_attr_value(elem, attr_name).as_deref() == Some(attr_value)
}

/// Get the value of an attribute by name.
pub(super) fn get_attr_value(elem: &HtmlElement, attr_name: &str) -> Option<String> {
    for (k, v) in &elem.attrs.0 {
        if k.resolve().as_str() == attr_name {
            return Some(v.to_string());
        }
    }
    None
}

/// Sanitize an HTML id into a valid Word bookmark/anchor name (letters, digits and
/// underscore; not starting with a digit; <= 40 chars).
pub(super) fn sanitize_anchor(id: &str) -> String {
    let mut out: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out.truncate(40);
    out
}

/// The `href` of the first `<a role="doc-biblioref">` within `nodes` — the link from
/// a citation marker to its bibliography entry.
pub(super) fn first_biblioref_href(nodes: &[HtmlNode]) -> Option<String> {
    find_first_element(nodes, &|element| {
        tag_name(element) == "a" && has_attr_value(element, "role", "doc-biblioref")
    })
    .and_then(|element| get_attr_value(element, "href"))
}

/// Collect each `<li>`'s `id` attribute within `nodes`, in document order — the
/// bibliography entries' anchors.
pub(super) fn collect_li_ids(nodes: &[HtmlNode], out: &mut Vec<Option<String>>) {
    for node in nodes {
        if let HtmlNode::Element(elem) = node {
            if tag_name(elem) == "li" {
                out.push(get_attr_value(elem, "id"));
            }
            collect_li_ids(&elem.children, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Alignment detection (I5)
// ---------------------------------------------------------------------------

/// Detect paragraph alignment from an HTML element's `style` attribute.
pub(super) fn detect_alignment(elem: &HtmlElement) -> Option<Alignment> {
    let style_val = get_attr_value(elem, "style")?;
    if style_val.contains("text-align: center") || style_val.contains("text-align:center") {
        Some(Alignment::Center)
    } else if style_val.contains("text-align: right") || style_val.contains("text-align:right") {
        Some(Alignment::Right)
    } else {
        None
    }
}

/// Recursively collect `Location`s of block-level `Tag::Start` nodes from
/// the HTML tree, preserving document order.  Only introspectable tags for
/// block-level elements (heading, par, equation, table, list, enum, figure,
/// image, section, outline) are collected — these are the tags whose page
/// numbers are meaningful for page-break detection.
pub(super) fn collect_block_tag_locations(children: &[HtmlNode], out: &mut Vec<Location>) {
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
