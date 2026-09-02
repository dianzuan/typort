use super::{
    Alignment, BlockElement, Content, Document, EquationElem, EquationState, HangingIndent,
    HtmlDocument, HtmlElement, HtmlNode, InlineElement, InlineFmt, InlineOptions, Location,
    Numbering, OutlineElem, Paragraph, ParagraphStyle, Run, Tag, WalkCtx, children_are_inline,
    collect_deep_text, collect_inlines, collect_li_ids, content_at_location, convert_html_list,
    convert_html_table, detect_alignment, element_at_location, find_img_src, find_tag_end,
    footnote, handle_heading, handle_list_tag, handle_table_tag, has_attr_value, image,
    is_block_equation, is_doc_endnotes_section, page, run_with_span, sanitize_anchor,
    subtree_has_element, tag_name,
};

/// Recursively walk `HtmlNode` children, dispatching on `Tag::Start` element types.
pub(super) fn walk_tags(children: &[HtmlNode], ctx: &mut WalkCtx) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    i = handle_block_tag(children, i, tag, content, ctx);
                }
                // Tag::End is consumed implicitly
            }
            HtmlNode::Element(elem) => {
                handle_html_element(elem, ctx);
            }
            HtmlNode::Text(text, span) => {
                // Bare text outside of any Tag — emit as a paragraph
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.push_run(run_with_span(trimmed, *span));
                    ctx.doc.add_paragraph(para);
                }
            }
            HtmlNode::Frame(frame) => {
                // Layouted-opaque content (a CeTZ canvas, curve art, a rotated
                // box): typst-html hands over the laid-out frame in document
                // order — rasterize it in place.
                if let Some(img) = image::rasterize_html_frame(frame) {
                    let mut para = Paragraph::new();
                    para.alignment = Some(Alignment::Center);
                    para.add_image(img);
                    ctx.doc.add_paragraph(para);
                }
            }
        }
        i += 1;
    }
}

/// Dispatch one block-level Typst tag and return the last consumed node index.
pub(super) fn handle_block_tag(
    children: &[HtmlNode],
    i: usize,
    tag: &Tag,
    content: &Content,
    ctx: &mut WalkCtx,
) -> usize {
    match content.elem().name() {
        "heading" => {
            if handle_heading(tag, ctx) == Some(1) {
                ctx.eq_state.chapter += 1;
                ctx.eq_state.eq_in_chapter = 0;
            }
            find_tag_end(children, i, tag.location())
        }
        "par" => handle_par_with_inline_equations(children, i, ctx),
        "equation" => {
            handle_equation(tag, ctx);
            find_tag_end(children, i, tag.location())
        }
        "footnote" => {
            handle_block_footnote(&children[i..], ctx.doc);
            find_tag_end(children, i, tag.location())
        }
        "table" => handle_table_tag(children, i, tag.location(), ctx),
        "list" => handle_list_tag(children, i, tag.location(), false, ctx),
        "enum" => handle_list_tag(children, i, tag.location(), true, ctx),
        "image" => handle_block_image(children, i, tag.location(), ctx),
        "figure" | "section" => handle_figure_or_section(children, i, tag, content, ctx),
        "outline" => handle_outline(children, i, tag.location(), ctx),
        // NOTE: no "pagebreak"/"colbreak" arms — in typst 0.15 both elements
        // carry a plain `#[elem]` (no Location), so explicit breaks are recovered
        // from the source AST (`breaks.rs`). Inline and unknown tags are skipped.
        _ => i,
    }
}

pub(super) fn handle_block_image(
    children: &[HtmlNode],
    i: usize,
    location: Location,
    ctx: &mut WalkCtx,
) -> usize {
    let end = find_tag_end(children, i, location);
    if let Some(src) = find_img_src(&children[i..=end])
        && let Some(img_data) = image::image_data_from_src(&src, ctx.image_sizes)
    {
        let mut para = Paragraph::new();
        para.add_image(img_data);
        ctx.doc.add_paragraph(para);
    }
    end
}

pub(super) fn handle_figure_or_section(
    children: &[HtmlNode],
    i: usize,
    tag: &Tag,
    content: &Content,
    ctx: &mut WalkCtx,
) -> usize {
    let location = tag.location();
    let end = find_tag_end(children, i, location);
    let is_figure = content.elem().name() == "figure";
    if !is_figure && is_doc_endnotes_section(&children[i..=end]) {
        return end;
    }

    if is_figure && let Some(label) = content.label() {
        let mut para = Paragraph::new();
        if ctx.add_bookmark(&mut para, label.resolve().to_string()) {
            ctx.doc.add_paragraph(para);
        }
    }

    let inner = &children[i + 1..end];
    // A vector-drawing body (#place'd curves, CeTZ) is dropped from the HTML
    // export entirely. Its raster is keyed by this figure's location.
    if is_figure
        && !subtree_has_element(inner, "table")
        && !subtree_has_element(inner, "image")
        && let Some(img) = ctx.figure_rasters.remove(&location)
    {
        let mut para = Paragraph::new();
        para.alignment = Some(Alignment::Center);
        para.add_image(img);
        ctx.doc.add_paragraph(para);
        emit_figure_caption(inner, ctx);
    } else {
        walk_tags(inner, ctx);
    }
    end
}

pub(super) fn handle_outline(
    children: &[HtmlNode],
    i: usize,
    location: Location,
    ctx: &mut WalkCtx,
) -> usize {
    let depth = element_at_location::<OutlineElem>(ctx.html_doc, location)
        .and_then(|outline| *outline.depth.as_option())
        .flatten()
        .map_or(3, |depth| u8::try_from(depth.get()).unwrap_or(3));
    let mut para = Paragraph::new();
    para.add_toc(depth);
    ctx.doc.add_paragraph(para);
    find_tag_end(children, i, location)
}

/// Emit only the `<figcaption>` element(s) in a figure subtree, skipping the
/// (rasterized) canvas body. Keeps the caption for vector-drawing figures while
/// dropping the canvas's leaked text labels.
pub(super) fn emit_figure_caption(nodes: &[HtmlNode], ctx: &mut WalkCtx) {
    for node in nodes {
        if let HtmlNode::Element(elem) = node {
            if tag_name(elem).as_str() == "figcaption" {
                handle_html_element(elem, ctx);
            } else {
                emit_figure_caption(&elem.children, ctx);
            }
        }
    }
}

/// Handle an HTML element (non-Tag node) — dispatches on element tag name.
pub(super) fn handle_html_element(elem: &HtmlElement, ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    let tag = tag_name(elem);
    match tag.as_str() {
        "pre" => convert_code_block(elem, ctx.doc),
        "blockquote" => convert_blockquote(elem, ctx),
        "dl" => convert_term_list(elem, ctx.doc),
        "ol" => convert_html_list(elem, ctx.doc, true, ctx.html_doc),
        "ul" => convert_html_list(elem, ctx.doc, false, ctx.html_doc),
        "table" => convert_html_table(elem, None, ctx.doc, html, ctx.world),
        "figcaption" => {
            // Collect all figcaption content into a single paragraph
            let mut para = Paragraph::new();
            para.alignment = Some(Alignment::Center);
            collect_inlines(
                &elem.children,
                &mut para,
                None,
                InlineOptions::generic(InlineFmt::default(), None),
            );
            if !para.inlines.is_empty() {
                ctx.doc.add_paragraph(para);
            }
        }
        // A block-level inline-formatting element that wraps an inline equation —
        // e.g. a whole `#emph[… $eq$ …]` body that is itself a block (as a custom
        // theorem/proof show rule produces). On typst 0.15 each inner inline
        // equation also emits a sibling `<math>` element; the old default
        // (`walk_tags`) walks this element as block content, splitting every text
        // node and inline equation into its own block paragraph (turning inline math
        // into stray display equations and dropping inter-word spaces). Collect it
        // as ONE paragraph instead, resolving the equations to OMML via the
        // introspector. Only do this when an equation is actually present, so the
        // ordinary block-emphasis case keeps its original run/spacing behavior.
        "em" | "i" | "strong" | "b" | "code" if subtree_has_element(&elem.children, "equation") => {
            let fmt = InlineFmt::default().for_tag(&tag);
            emit_inline_equation_paragraph(elem, ctx, fmt, None);
        }
        "section" => {
            // Skip doc-endnotes section
            if has_attr_value(elem, "role", "doc-endnotes") {
                return;
            }
            if has_attr_value(elem, "role", "doc-bibliography") {
                let start_idx = ctx.doc.body.elements.len();
                walk_tags(&elem.children, ctx);
                let bib_elements: Vec<_> = ctx.doc.body.elements.drain(start_idx..).collect();
                // Each reference `<li id="loc-N">` becomes one entry paragraph in
                // order; bookmark each by its id so citations can link to it.
                let mut li_ids = Vec::new();
                collect_li_ids(&elem.children, &mut li_ids);
                let mut bib_paragraphs = Vec::new();
                let mut entry_idx = 0;
                for element in bib_elements {
                    match element {
                        BlockElement::Paragraph(p) => {
                            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                                ctx.doc.add_paragraph(p);
                            } else {
                                let mut bp = p;
                                bp.hanging_indent = Some(HangingIndent::Default);
                                // Typst emits the reference list as a <ul>, so each
                                // entry arrived tagged as a bullet list item. The
                                // "[n]" label is already the marker — drop the list
                                // so Word doesn't prepend a redundant bullet; the
                                // hanging indent above gives the reference layout.
                                bp.list_info = None;
                                if let Some(Some(id)) = li_ids.get(entry_idx) {
                                    let bk_id = ctx.doc.next_bookmark_id();
                                    bp.add_bookmark_at_start(bk_id, sanitize_anchor(id));
                                }
                                entry_idx += 1;
                                bib_paragraphs.push(bp);
                            }
                        }
                        other => {
                            ctx.doc.body.elements.push(other);
                        }
                    }
                }
                if !bib_paragraphs.is_empty() {
                    ctx.doc.body.elements.push(BlockElement::BibliographyBlock {
                        paragraphs: bib_paragraphs,
                    });
                }
                return;
            }
            walk_tags(&elem.children, ctx);
        }
        // A block container whose direct children are purely inline content
        // (text + inline-format spans + inline equations) and that holds at least
        // one inline equation — e.g. the body of a custom theorem/proof show rule
        // (`block[ … $eq$ … ]`). `walk_tags` would treat each bare text node and
        // each inline equation as its own block paragraph (and on typst 0.15 turn
        // the inline math into stray display equations), so collect it as ONE
        // paragraph here, resolving the equations to OMML via the introspector.
        // Gated on an inline equation being present so ordinary inline-content
        // blocks keep their original handling.
        _ if children_are_inline(&elem.children)
            && subtree_has_element(&elem.children, "equation") =>
        {
            emit_inline_equation_paragraph(elem, ctx, InlineFmt::default(), detect_alignment(elem));
        }
        _ => {
            // Check for alignment on this element and apply to child paragraphs
            let alignment = detect_alignment(elem);
            let start_idx = ctx.doc.body.elements.len();
            walk_tags(&elem.children, ctx);
            if let Some(align) = alignment {
                for element in &mut ctx.doc.body.elements[start_idx..] {
                    if let BlockElement::Paragraph(para) = element {
                        para.alignment = Some(align);
                    }
                }
            }
        }
    }
}

/// Collect `elem`'s children into a single paragraph (text + inline equations
/// resolved to OMML via the introspector) and emit it. Used for block-level
/// inline-content containers whose inner equations must stay inline rather than be
/// promoted to display equations by `walk_tags`.
pub(super) fn emit_inline_equation_paragraph(
    elem: &HtmlElement,
    ctx: &mut WalkCtx,
    fmt: InlineFmt,
    alignment: Option<Alignment>,
) {
    let mut para = Paragraph::new();
    para.alignment = alignment;
    collect_inlines(
        &elem.children,
        &mut para,
        None,
        InlineOptions::generic(fmt, Some(ctx.html_doc)),
    );
    if !para.inlines.is_empty() {
        ctx.doc.add_paragraph(para);
    }
}

/// Handle a `par` Tag: collect inline children (text, strong, emph, equation, footnote)
/// and emit a paragraph.
pub(super) fn handle_par(slice: &[HtmlNode], ctx: &mut WalkCtx) {
    let mut para = Paragraph::new();
    // Skip the first Tag::Start("par") and collect inlines from the inner nodes
    let inner = &slice[1..slice.len().saturating_sub(1)];
    collect_inlines(inner, &mut para, Some(ctx), InlineOptions::paragraph());
    if !para.inlines.is_empty() {
        strip_cjk_spaces(&mut para);
        ctx.doc.add_paragraph(para);
    }
}

/// Handle a `par` tag at position `par_start` in `children`, merging subsequent
/// inline equations and continuation `par` fragments into a single paragraph.
///
/// Typst's HTML output splits paragraphs around inline equations:
///   par("Text with") -> equation($x$) -> par("more text") -> equation($y$) -> par("end.")
/// This function detects that pattern and merges everything into one `<w:p>`.
///
/// Returns the index of the last consumed node (the caller's loop will `i += 1`).
pub(super) fn handle_par_with_inline_equations(
    children: &[HtmlNode],
    par_start: usize,
    ctx: &mut WalkCtx,
) -> usize {
    let html = ctx.html_doc;
    let HtmlNode::Tag(tag) = &children[par_start] else {
        return par_start;
    };
    let par_end = find_tag_end(children, par_start, tag.location());

    // Check if the next sibling after this par is an inline equation.
    // If not, just handle as a normal paragraph (fast path).
    let next_start = par_end + 1;
    if !is_inline_equation_at(children, next_start, html) {
        handle_par(&children[par_start..=par_end], ctx);
        return par_end;
    }

    // Merge mode: build a single paragraph from par + inline eq + par + ...
    let mut para = Paragraph::new();

    // Collect inlines from the first par fragment
    let inner = &children[par_start + 1..par_end];
    collect_inlines(inner, &mut para, Some(ctx), InlineOptions::paragraph());

    // The pattern is strictly: equation -> par -> equation -> par -> ...
    // After each inline equation, we expect a continuation par.
    // After each continuation par, we ONLY continue if the next thing is
    // another inline equation (otherwise this par is actually a new paragraph).
    let mut cursor = next_start;
    while cursor < children.len() {
        // Step 1: expect an inline equation
        if !is_inline_equation_at(children, cursor, html) {
            break;
        }
        if let HtmlNode::Tag(eq_tag) = &children[cursor] {
            let loc = eq_tag.location();
            if let Some(c) = content_at_location(html, loc) {
                para.push_run(Run::new(" "));
                let omml = typort_math::equation_to_omml(&c);
                para.add_math(omml);
            }
            cursor = find_tag_end(children, cursor, loc) + 1;
        } else {
            break;
        }

        // Step 2: expect a continuation par
        if !is_par_tag_at(children, cursor) {
            break;
        }
        if let HtmlNode::Tag(pt) = &children[cursor] {
            let p_end = find_tag_end(children, cursor, pt.location());
            let p_inner = &children[cursor + 1..p_end];
            para.push_run(Run::new(" "));
            collect_inlines(p_inner, &mut para, Some(ctx), InlineOptions::paragraph());
            cursor = p_end + 1;
        } else {
            break;
        }
        // Loop back: if next is another inline equation, continue merging.
        // Otherwise, the loop condition will break out.
    }

    if !para.inlines.is_empty() {
        strip_cjk_spaces(&mut para);
        ctx.doc.add_paragraph(para);
    }

    // Return index of last consumed node (cursor - 1 since the outer loop does i += 1)
    cursor.saturating_sub(1)
}

pub(super) fn strip_cjk_spaces(para: &mut Paragraph) {
    let mut remove_indices = Vec::new();
    for i in 1..para.inlines.len().saturating_sub(1) {
        let InlineElement::Text(run) = &para.inlines[i] else {
            continue;
        };
        if run.text.trim() != "" {
            continue;
        }
        let prev = &para.inlines[i - 1];
        let next = &para.inlines[i + 1];
        let prev_ends_cjk = matches!(prev, InlineElement::Text(r)
            if r.text.chars().last().is_some_and(page::is_cjk_char));
        let next_starts_cjk = matches!(next, InlineElement::Text(r)
            if r.text.chars().next().is_some_and(page::is_cjk_char));
        let prev_is_math = matches!(prev, InlineElement::Math { .. });
        let next_is_math = matches!(next, InlineElement::Math { .. });
        // A space adjacent to CJK on one side carries no meaning when the other
        // side is CJK text or an inline equation — Chinese needs no separator from
        // a neighbouring character or formula. (A space between Latin text and an
        // equation IS kept: Typst trims the source space and Word needs it back,
        // e.g. "the value x is".)
        if (prev_ends_cjk && (next_starts_cjk || next_is_math)) || (prev_is_math && next_starts_cjk)
        {
            remove_indices.push(i);
        }
    }
    for idx in remove_indices.into_iter().rev() {
        para.inlines.remove(idx);
    }
}

pub(super) fn strip_visual_markers(s: &str) -> String {
    let trimmed = s.trim_start_matches(['•', '‣', '◦', '▪', '▸', '–', '—']);
    let trimmed = trimmed.trim_start();
    // Strip leading "1." or "1.1" or "1.1.1" numbering patterns
    let trimmed = if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        rest.trim_start()
    } else {
        trimmed
    };
    trimmed.to_string()
}

pub(super) fn strip_cjk_spaces_str(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' '
            && i > 0
            && i + 1 < chars.len()
            && page::is_cjk_char(chars[i - 1])
            && page::is_cjk_char(chars[i + 1])
        {
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Check if position `idx` in `children` is a `Tag::Start("equation")` for an
/// **inline** (non-block) equation.
pub(super) fn is_inline_equation_at(
    children: &[HtmlNode],
    idx: usize,
    html_doc: &HtmlDocument,
) -> bool {
    let Some(HtmlNode::Tag(tag)) = children.get(idx) else {
        return false;
    };
    let Tag::Start(content, _) = tag else {
        return false;
    };
    if content.elem().name() != "equation" {
        return false;
    }
    // Check that it's an inline equation (block == false)
    let loc = tag.location();
    let Some(content) = content_at_location(html_doc, loc) else {
        return false;
    };
    !is_block_equation(&content)
}

/// Check if position `idx` in `children` is a `Tag::Start("par")`.
pub(super) fn is_par_tag_at(children: &[HtmlNode], idx: usize) -> bool {
    let Some(HtmlNode::Tag(tag)) = children.get(idx) else {
        return false;
    };
    let Tag::Start(content, _) = tag else {
        return false;
    };
    content.elem().name() == "par"
}
/// Handle a block-level equation Tag.
pub(super) fn handle_equation(tag: &Tag, ctx: &mut WalkCtx) {
    let Some(content) = content_at_location(ctx.html_doc, tag.location()) else {
        return;
    };

    if is_block_equation(&content) {
        emit_block_equation(&content, ctx);
    } else {
        // Inline equation at block level: wrap in a paragraph
        let mut para = Paragraph::new();
        let omml = typort_math::equation_to_omml(&content);
        para.add_math(omml);
        ctx.doc.add_paragraph(para);
    }
}

/// Emit a labelled and optionally numbered block equation as one paragraph.
pub(super) fn emit_block_equation(content: &Content, ctx: &mut WalkCtx) {
    let mut para = Paragraph::new();
    if let Some(label) = content.label() {
        ctx.add_bookmark(&mut para, label.resolve().to_string());
    }
    let number = compute_equation_number(content.to_packed::<EquationElem>(), ctx.eq_state);
    let omml = typort_math::equation_to_omml(content);
    if let Some(number) = number {
        para.add_numbered_math(omml, number);
    } else {
        para.add_math(omml);
    }
    ctx.doc.add_paragraph(para);
}

/// Handle a block-level footnote Tag.
pub(super) fn handle_block_footnote(children_from_here: &[HtmlNode], doc: &mut Document) {
    let footnote_id = footnote::find_footnote_id_in_range(children_from_here);
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
}

/// Convert a `<pre>` code block into monospace paragraphs (one per line).
pub(super) fn convert_code_block(elem: &HtmlElement, doc: &mut Document) {
    let text = collect_deep_text(&elem.children);
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
pub(super) fn convert_blockquote(elem: &HtmlElement, ctx: &mut WalkCtx) {
    let start_idx = ctx.doc.body.elements.len();
    walk_tags(&elem.children, ctx);
    // Typst quote block default pad = 1em per side
    let indent_twips = ctx.doc.style.body_size_half_pt * 10;
    for element in &mut ctx.doc.body.elements[start_idx..] {
        if let BlockElement::Paragraph(para) = element {
            para.left_indent = Some(indent_twips);
            para.suppress_indent = true;
        }
    }
}

/// Convert a `<dl>` (definition list) into bold terms and indented definitions.
pub(super) fn convert_term_list(elem: &HtmlElement, doc: &mut Document) {
    for child in &elem.children {
        if let HtmlNode::Element(item) = child {
            let tag = tag_name(item);
            match tag.as_str() {
                "dt" => {
                    let mut para = Paragraph::new();
                    para.suppress_indent = true;
                    collect_inlines(
                        &item.children,
                        &mut para,
                        None,
                        InlineOptions::generic(InlineFmt::bold(), None),
                    );
                    if !para.inlines.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                "dd" => {
                    let mut para = Paragraph::new();
                    para.left_indent = Some(doc.style.first_line_indent_twips);
                    para.suppress_indent = true;
                    collect_inlines(
                        &item.children,
                        &mut para,
                        None,
                        InlineOptions::generic(InlineFmt::default(), None),
                    );
                    if !para.inlines.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                _ => {}
            }
        }
    }
}
/// Compute the equation number string for a block equation, if it has numbering.
pub(super) fn compute_equation_number(
    eq_packed: Option<&typst::foundations::Packed<EquationElem>>,
    eq_state: &mut EquationState,
) -> Option<String> {
    let eq = eq_packed?;
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
        // `NumberingPattern::apply` now takes a `warning_context` and returns a
        // `StrResult`; pass `None` (no engine to warn through here) and drop a
        // formatting error rather than panicking.
        pattern.apply(None, &nums).ok().map(|s| s.to_string())
    } else {
        None
    }
}
