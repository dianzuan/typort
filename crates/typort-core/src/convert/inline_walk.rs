use super::{
    Alignment, CiteElem, CiteGroup, EmphElem, EquationElem, HtmlDocument, HtmlElement, HtmlNode,
    InlineFmt, LinkElem, Paragraph, RefElem, Run, StrongElem, Tag, WalkCtx, collect_flat_text,
    content_at_location, drain_text_runs, element_at_location, emit_block_equation, find_img_src,
    find_tag_end, first_biblioref_href, footnote, get_attr_value, has_attr_value, image, inline,
    is_block_equation, run_with_span, sanitize_anchor, subtree_has_element, tag_name,
};

/// The behavior needed by a caller of the shared HTML inline collector.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InlinePurpose {
    Paragraph,
    Generic,
    FormattedRun,
    Footnote,
}

/// Options for the single HTML inline collector.
#[derive(Clone, Copy)]
pub(super) struct InlineOptions<'a> {
    fmt: InlineFmt,
    html_doc: Option<&'a HtmlDocument>,
    purpose: InlinePurpose,
}

impl<'a> InlineOptions<'a> {
    pub(super) fn paragraph() -> Self {
        Self {
            fmt: InlineFmt::default(),
            html_doc: None,
            purpose: InlinePurpose::Paragraph,
        }
    }

    pub(super) fn generic(fmt: InlineFmt, html_doc: Option<&'a HtmlDocument>) -> Self {
        Self {
            fmt,
            html_doc,
            purpose: InlinePurpose::Generic,
        }
    }

    pub(super) fn formatted_run() -> Self {
        Self {
            fmt: InlineFmt::default(),
            html_doc: None,
            purpose: InlinePurpose::FormattedRun,
        }
    }

    pub(super) fn footnote(fmt: InlineFmt) -> Self {
        Self {
            fmt,
            html_doc: None,
            purpose: InlinePurpose::Footnote,
        }
    }

    pub(super) fn with_fmt(self, fmt: InlineFmt) -> Self {
        Self { fmt, ..self }
    }
}

/// Collect inline elements using the exact behavior selected by `options`.
pub(super) fn collect_inlines(
    children: &[HtmlNode],
    para: &mut Paragraph,
    mut ctx: Option<&mut WalkCtx<'_>>,
    options: InlineOptions<'_>,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Text(text, span) => {
                if !text.is_empty() {
                    let mut run = if options.purpose == InlinePurpose::Footnote {
                        Run::new(text.as_str())
                    } else {
                        run_with_span(text.as_str(), *span)
                    };
                    options.fmt.apply_to(&mut run);
                    para.push_run(run);
                }
            }
            HtmlNode::Tag(tag) => {
                if let Tag::Start(..) = tag {
                    match options.purpose {
                        InlinePurpose::Paragraph => {
                            if let Some(ctx) = ctx.as_deref_mut() {
                                i = handle_inline_tag(tag, children, i, ctx, para);
                            }
                        }
                        InlinePurpose::Generic => {
                            collect_generic_inline_tag(children, i, tag, para, options);
                        }
                        InlinePurpose::FormattedRun => {
                            i = collect_formatted_inline_tag(children, i, tag, para, options);
                        }
                        InlinePurpose::Footnote => collect_footnote_inline_tag(tag, para),
                    }
                }
            }
            HtmlNode::Element(elem) => {
                collect_inline_element(elem, para, ctx.as_deref_mut(), options);
            }
            HtmlNode::Frame(frame) => {
                if matches!(
                    options.purpose,
                    InlinePurpose::Paragraph | InlinePurpose::Generic
                ) {
                    // Layouted-opaque inline content (e.g. a boxed drawing as a
                    // figure body): rasterize in place as an inline image.
                    if let Some(img) = image::rasterize_html_frame(frame) {
                        para.add_image(img);
                    }
                }
            }
        }
        i += 1;
    }
}

pub(super) fn collect_inline_element(
    elem: &HtmlElement,
    para: &mut Paragraph,
    ctx: Option<&mut WalkCtx<'_>>,
    options: InlineOptions<'_>,
) {
    match options.purpose {
        InlinePurpose::Paragraph => {
            if let Some(ctx) = ctx {
                handle_inline_html_element(elem, ctx, para);
            }
        }
        InlinePurpose::Generic => {
            if tag_name(elem) == "math" || has_attr_value(elem, "role", "doc-noteref") {
                return;
            }
            let fmt = options.fmt.for_tag(&tag_name(elem));
            collect_inlines(&elem.children, para, None, options.with_fmt(fmt));
        }
        InlinePurpose::FormattedRun => {
            // Deliberately descend into MathML here. Link display collection has
            // historically leaked its glyphs; changing that output belongs to #12.
            let fmt = options.fmt.for_tag(&tag_name(elem));
            collect_inlines(&elem.children, para, None, options.with_fmt(fmt));
        }
        InlinePurpose::Footnote => {
            if tag_name(elem) == "math" || has_attr_value(elem, "role", "doc-backlink") {
                return;
            }
            let fmt = options.fmt.for_tag(&tag_name(elem));
            collect_inlines(&elem.children, para, None, options.with_fmt(fmt));
        }
    }
}

pub(super) fn collect_generic_inline_tag(
    children: &[HtmlNode],
    i: usize,
    tag: &Tag,
    para: &mut Paragraph,
    options: InlineOptions<'_>,
) {
    let Tag::Start(content, _) = tag else { return };
    match content.elem().name() {
        "footnote" => {
            if let Some(id) = footnote::find_footnote_id_in_range(&children[i..]) {
                para.add_footnote_ref(id + 1);
            }
        }
        "equation" => {
            if let Some(html_doc) = options.html_doc
                && let Some(content) = content_at_location(html_doc, tag.location())
            {
                para.add_math(typort_math::equation_to_omml(&content));
            }
        }
        _ => {}
    }
}

pub(super) fn collect_formatted_inline_tag(
    children: &[HtmlNode],
    i: usize,
    tag: &Tag,
    para: &mut Paragraph,
    options: InlineOptions<'_>,
) -> usize {
    let Tag::Start(content, _) = tag else {
        return i;
    };
    let end = find_tag_end(children, i, tag.location());
    let fmt = match content.elem().name() {
        "strong" | "emph" => options.fmt.for_tag(content.elem().name()),
        "raw" => InlineFmt {
            monospace: true,
            ..options.fmt
        },
        _ => options.fmt,
    };
    collect_inlines(&children[i + 1..end], para, None, options.with_fmt(fmt));
    end
}

pub(super) fn collect_footnote_inline_tag(tag: &Tag, para: &mut Paragraph) {
    if let Tag::Start(content, _) = tag
        && content.elem().name() == "equation"
    {
        para.add_math(typort_math::equation_to_omml(content));
    }
}

/// Whether `content` (recursively) contains an `EquationElem`.
///
/// Used to decide whether an emphasis/strong body must be descended through the
/// equation-aware DOM walk (typst 0.15 nests inline math inside emphasis) rather
/// than the run-only `inline::extract_runs` fast path.
pub(super) fn content_has_equation(content: &typst::foundations::Content) -> bool {
    use typst_library::foundations::SequenceElem;
    use typst_library::model::{EmphElem, StrongElem};

    if content.to_packed::<EquationElem>().is_some() {
        true
    } else if let Some(seq) = content.to_packed::<SequenceElem>() {
        seq.children.iter().any(content_has_equation)
    } else if let Some(s) = content.to_packed::<StrongElem>() {
        content_has_equation(&s.body)
    } else if let Some(e) = content.to_packed::<EmphElem>() {
        content_has_equation(&e.body)
    } else {
        false
    }
}

/// Process a single inline `Tag::Start` within a paragraph.
/// Returns the new index (pointing at the matching End tag).
pub(super) fn handle_inline_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let Tag::Start(content, _) = tag else {
        return i;
    };
    match content.elem().name() {
        "strong" | "emph" => handle_emphasis_tag(tag, children, i, ctx, para),
        "equation" => handle_inline_equation_tag(tag, children, i, ctx, para),
        "footnote" => handle_inline_footnote_tag(tag, children, i, para),
        "image" => handle_inline_image_tag(tag, children, i, ctx, para),
        "ref" => handle_inline_ref_tag(tag, children, i, ctx, para),
        "link" => handle_inline_link_tag(tag, children, i, ctx, para),
        "super" | "sub" | "raw" | "underline" | "strike" | "highlight" | "overline"
        | "smallcaps" => handle_inline_format_tag(tag, children, i, para),
        "cite-group" => handle_inline_cite_group_tag(tag, children, i, ctx, para),
        "par" | "context" => handle_nested_inline_tag(tag, children, i, ctx, para),
        "caption" => handle_inline_caption_tag(tag, children, i, ctx, para),
        _ => find_tag_end(children, i, tag.location()),
    }
}

pub(super) fn handle_emphasis_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    let Tag::Start(content, _) = tag else {
        return end;
    };
    let (body, fmt) = if content.elem().name() == "strong" {
        (
            element_at_location::<StrongElem>(ctx.html_doc, tag.location())
                .map(|strong| strong.body.clone()),
            InlineFmt::bold(),
        )
    } else {
        (
            element_at_location::<EmphElem>(ctx.html_doc, tag.location())
                .map(|emph| emph.body.clone()),
            InlineFmt::italic(),
        )
    };

    // `extract_runs` only carries text. Descend through the equation-aware HTML
    // collector when emphasis wraps math; retain the cheaper Content walk otherwise.
    if body.as_ref().is_some_and(content_has_equation) {
        let mut tmp = Paragraph::new();
        collect_inlines(
            &children[i + 1..end],
            &mut tmp,
            None,
            InlineOptions::generic(fmt, Some(ctx.html_doc)),
        );
        para.inlines.append(&mut tmp.inlines);
    } else if let Some(body) = body {
        for mut run in inline::extract_runs(&body) {
            if fmt.bold {
                run.bold = true;
            } else {
                run.italic = true;
            }
            para.push_run(run);
        }
    }
    end
}

pub(super) fn handle_inline_equation_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    if let Some(content) = content_at_location(ctx.html_doc, tag.location()) {
        if is_block_equation(&content) {
            if !para.inlines.is_empty() {
                ctx.doc.add_paragraph(std::mem::take(para));
            }
            emit_block_equation(&content, ctx);
        } else {
            para.add_math(typort_math::equation_to_omml(&content));
        }
    }
    find_tag_end(children, i, tag.location())
}

pub(super) fn handle_inline_footnote_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    para: &mut Paragraph,
) -> usize {
    if let Some(id) = footnote::find_footnote_id_in_range(&children[i..]) {
        para.add_footnote_ref(id + 1);
    }
    find_tag_end(children, i, tag.location())
}

pub(super) fn handle_inline_image_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    if let Some(src) = find_img_src(&children[i..=end])
        && let Some(img_data) = image::image_data_from_src(&src, ctx.image_sizes)
    {
        para.add_image(img_data);
    }
    end
}

pub(super) fn handle_inline_ref_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    if let Some(reference) = element_at_location::<RefElem>(ctx.html_doc, tag.location()) {
        let target = reference.target.resolve().to_string();
        let display = collect_flat_text(&children[i + 1..end]);
        if ctx.bib_keys.contains(&target) {
            let mut run = Run::new(&display);
            run.superscript = subtree_has_element(&children[i + 1..end], "sup");
            match first_biblioref_href(&children[i + 1..end]) {
                Some(href) => {
                    para.add_internal_link(
                        sanitize_anchor(href.trim_start_matches('#')),
                        vec![run],
                    );
                }
                None => para.push_run(run),
            }
        } else {
            para.add_field_ref(target, display);
        }
    }
    end
}

pub(super) fn handle_inline_link_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    let Some(link) = element_at_location::<LinkElem>(ctx.html_doc, tag.location()) else {
        return end;
    };
    let typst_library::model::LinkTarget::Dest(typst_library::model::Destination::Url(url)) =
        &link.dest
    else {
        return end;
    };
    let mut display = Paragraph::new();
    collect_inlines(
        &children[i + 1..end],
        &mut display,
        None,
        InlineOptions::formatted_run(),
    );
    let runs = drain_text_runs(&mut display);
    if !runs.is_empty() {
        para.add_hyperlink(url.to_string(), runs);
    }
    end
}

pub(super) fn handle_inline_format_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    let text = collect_flat_text(&children[i + 1..end]);
    if !text.is_empty() {
        let mut run = Run::new(&text);
        if let Tag::Start(content, _) = tag {
            apply_inline_format(content.elem().name(), &mut run);
        }
        para.push_run(run);
    }
    end
}

pub(super) fn handle_inline_cite_group_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    if let Some(cite_group) = element_at_location::<CiteGroup>(ctx.html_doc, tag.location()) {
        let keys = cite_group
            .children
            .iter()
            .filter_map(|cite| cite.to_packed::<CiteElem>())
            .map(|cite| cite.key.resolve().to_string())
            .collect::<Vec<_>>();
        let display = collect_flat_text(&children[i + 1..end]);
        if !keys.is_empty() && !display.is_empty() {
            para.add_citation(keys, display);
        }
    }
    end
}

pub(super) fn handle_nested_inline_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    collect_inlines(
        &children[i + 1..end],
        para,
        Some(ctx),
        InlineOptions::paragraph(),
    );
    end
}

pub(super) fn handle_inline_caption_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let end = find_tag_end(children, i, tag.location());
    let text = collect_flat_text(&children[i + 1..end]);
    if !text.trim().is_empty() {
        if !para.inlines.is_empty() {
            ctx.doc.add_paragraph(std::mem::take(para));
        }
        let mut caption = Paragraph::new();
        caption.alignment = Some(Alignment::Center);
        caption.push_run(Run::new(text.trim()));
        ctx.doc.add_paragraph(caption);
    }
    end
}

/// Apply the appropriate formatting flag to a `Run` based on the inline tag name.
pub(super) fn apply_inline_format(tag_name: &str, run: &mut Run) {
    match tag_name {
        "super" => run.superscript = true,
        "sub" => run.subscript = true,
        "raw" => run.monospace = true,
        "underline" | "overline" => run.underline = true,
        "strike" => run.strikethrough = true,
        "highlight" => run.highlight_color = Some("yellow".into()),
        "smallcaps" => run.smallcaps = true,
        _ => {}
    }
}

/// Process a single inline HTML element within a paragraph.
pub(super) fn handle_inline_html_element(
    elem: &HtmlElement,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) {
    let tag_str = tag_name(elem);
    match tag_str.as_str() {
        "strong" | "b" | "em" | "i" => {
            // Pass the doc and move ALL inlines (not just text runs): emphasis can
            // wrap an inline equation (typst 0.15 nests math inside the emphasis
            // element), which the shared inline collector emits as OMML via the
            // introspector — `drain_text_runs` would silently drop that math.
            let mut tmp = Paragraph::new();
            collect_inlines(
                &elem.children,
                &mut tmp,
                None,
                InlineOptions::generic(
                    InlineFmt::default().for_tag(tag_str.as_str()),
                    Some(ctx.html_doc),
                ),
            );
            para.inlines.append(&mut tmp.inlines);
        }
        "code" => {
            let mut tmp = Paragraph::new();
            collect_inlines(
                &elem.children,
                &mut tmp,
                None,
                InlineOptions::generic(
                    InlineFmt {
                        monospace: true,
                        ..InlineFmt::default()
                    },
                    Some(ctx.html_doc),
                ),
            );
            para.inlines.append(&mut tmp.inlines);
        }
        "math" => {
            // typst 0.15 emits an inline equation as a native MathML `<math>`
            // element ALONGSIDE the `Tag::Start("equation")` introspection marker.
            // The equation handler already produced OMML from that marker, so skip
            // the `<math>` element — walking it would re-emit the equation's glyphs
            // as literal duplicate text.
        }
        _ if has_attr_value(elem, "role", "doc-noteref") => {
            // The footnote reference marker. typst 0.15 puts the role on the
            // wrapping `<sup>` (0.14 used `<a>`), so match by role, not tag name.
            // Already emitted by the `Tag::Start("footnote")` handler; skip its
            // text so the number isn't also rendered as literal superscript.
        }
        "a" => {
            // External hyperlink from HTML <a href="...">
            if let Some(href) = get_attr_value(elem, "href") {
                let mut tmp = Paragraph::new();
                collect_inlines(
                    &elem.children,
                    &mut tmp,
                    None,
                    InlineOptions::generic(InlineFmt::default(), None),
                );
                let runs = drain_text_runs(&mut tmp);
                if !runs.is_empty() {
                    para.add_hyperlink(href, runs);
                }
            } else {
                collect_inlines(&elem.children, para, Some(ctx), InlineOptions::paragraph());
            }
        }
        "sup" => {
            let mut tmp = Paragraph::new();
            collect_inlines(
                &elem.children,
                &mut tmp,
                None,
                InlineOptions::generic(InlineFmt::default(), None),
            );
            for mut run in drain_text_runs(&mut tmp) {
                run.superscript = true;
                para.push_run(run);
            }
        }
        "sub" => {
            let mut tmp = Paragraph::new();
            collect_inlines(
                &elem.children,
                &mut tmp,
                None,
                InlineOptions::generic(InlineFmt::default(), None),
            );
            for mut run in drain_text_runs(&mut tmp) {
                run.subscript = true;
                para.push_run(run);
            }
        }
        "br" => {
            // A forced line break (`\`) — without this it falls into the default arm
            // (no children) and the surrounding words glue together.
            para.push_run(Run::line_break());
        }
        _ => {
            collect_inlines(&elem.children, para, Some(ctx), InlineOptions::paragraph());
        }
    }
}
