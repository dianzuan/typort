//! Tag-walker based Typst -> OOXML conversion (v2).
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

pub mod inline;
pub mod page;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;

use typort_ooxml::document::{
    Alignment, BlockElement, CellContent, Document, FootnoteFormat, ImageData, ImageFormat,
    InlineElement, Paragraph, ParagraphStyle, Run, Table, TableCell, TableRow, VMerge,
};
use typst::foundations::StyleChain;
use typst::introspection::{Location, Tag};
use typst::layout::{Frame, FrameItem, PagedDocument, Point};
use typst::model::Numbering;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;
use typst_library::model::{HeadingElem, OutlineElem, RefElem};

/// Tracks equation numbering state across the document.
#[derive(Default)]
struct EquationState {
    /// Current chapter number (incremented on h1 headings).
    chapter: u64,
    /// Equation counter within the current chapter.
    eq_in_chapter: u64,
    /// Global equation counter.
    global_eq: u64,
}

use crate::world::TyportWorld;

/// Rowspan metadata for a single cell: `(html_cell_index, rowspan, colspan)`.
type CellSpanInfo = (usize, u32, u32);

/// A parsed table row paired with its rowspan metadata.
type RawTableRow = (TableRow, Vec<CellSpanInfo>);

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

    // 3. Extract page settings and document style from PagedDocument (heuristic)
    if let Some(paged) = &paged_doc {
        doc.style = page::extract_document_style(paged);
        page::extract_page_settings(paged, &mut doc.page_settings);

        // 3a. Detect columns from page layout (heuristic fallback)
        if let Some(cols) = page::detect_columns(paged) {
            doc.page_settings.columns = Some(cols);
        }
    }

    // 3b. Override with authoritative values from source AST
    let source_overrides =
        page::extract_source_style_overrides(world.main_source().text());
    apply_source_overrides(&source_overrides, &mut doc);

    // 4. First pass: extract footnote content from <section role="doc-endnotes">
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);
    let footnote_contents = extract_footnote_contents(&body.children);
    for content in &footnote_contents {
        doc.add_footnote(content.clone());
    }

    // 5. Extract images from PagedDocument for embedding
    let mut image_queue: VecDeque<ImageData> = if let Some(paged) = &paged_doc {
        extract_images_from_paged(paged).into()
    } else {
        VecDeque::new()
    };

    // 6. Pre-compute page break locations from PagedDocument introspector
    let page_breaks: HashSet<Location> = if let Some(paged) = &paged_doc {
        collect_page_break_locations(&body.children, paged)
    } else {
        HashSet::new()
    };

    // 7. Walk the HTML tree's Tag sequence (page breaks are inserted inline
    //    based on the pre-computed page_breaks set from step 6).
    let mut eq_state = EquationState::default();
    let mut bookmarks: HashSet<String> = HashSet::new();
    walk_tags(
        &body.children,
        &html_doc,
        &mut doc,
        &mut eq_state,
        &mut image_queue,
        &mut bookmarks,
        &page_breaks,
    );

    // 8. Detect footnote format (circled numbers)
    detect_footnote_format(&body.children, &mut doc);

    // 9. Extract headers and footers from PagedDocument (before content
    //    recovery so header/footer text is not misidentified as missing body content)
    if let Some(paged) = &paged_doc {
        if doc.header.is_none() {
            doc.header = page::extract_header(paged);
        }
        // 9a. Detect page numbering before extracting footer.
        // If the footer is just a page number, set page_numbering instead of
        // static footer text, so the writer generates a PAGE field code.
        if let Some(fmt) = page::detect_page_numbering(paged) {
            doc.page_numbering = Some(fmt);
            // Don't set doc.footer — the writer will generate a PAGE field footer
        } else if doc.footer.is_none() {
            doc.footer = page::extract_footer(paged);
        }
    }

    // 10. Recover missing content from PagedDocument (e.g. #align(center) blocks)
    if let Some(paged) = &paged_doc {
        recover_missing_content(paged, &mut doc);
    }

    // 11. Extract title/author from document metadata, falling back to first heading
    extract_document_metadata(&html_doc, &mut doc);

    // 12. Post-processing: suppress indent after headings, bibliography hanging indent
    apply_paragraph_formatting(&mut doc);

    // 12a. Post-processing: apply per-run styles (color, font, size, bold,
    //       italic) and heading alignment from PagedDocument
    if let Some(paged) = &paged_doc {
        page::apply_styles_from_paged(paged, &mut doc);
    }

    // 12c. Post-processing: detect small caps from source text
    apply_smallcaps_from_source(world, &mut doc);

    // 13. Detect and apply section breaks from page setting changes
    if let Some(paged) = &paged_doc {
        let sections = page::detect_section_breaks(paged);
        if !sections.is_empty() {
            page::apply_section_breaks(&mut doc, &sections, paged.pages.len());
        }
    }

    // 14. Detect horizontal rules from PagedDocument and insert them
    if let Some(paged) = &paged_doc {
        insert_horizontal_rules_from_paged(paged, &mut doc);
    }

    Ok(doc)
}

/// Apply authoritative values from source AST, overriding heuristic guesses.
fn apply_source_overrides(
    ovr: &page::SourceStyleOverrides,
    doc: &mut Document,
) {
    // Page margins
    if let Some(v) = ovr.margin_top { doc.page_settings.margin_top = v; }
    if let Some(v) = ovr.margin_bottom { doc.page_settings.margin_bottom = v; }
    if let Some(v) = ovr.margin_left { doc.page_settings.margin_left = v; }
    if let Some(v) = ovr.margin_right { doc.page_settings.margin_right = v; }

    // Columns
    if let Some(cols) = ovr.columns {
        doc.page_settings.columns = Some(cols);
    }

    // Body text font
    if let Some(fonts) = &ovr.text_font
        && let Some(f) = fonts.first()
    {
        doc.style.body_font_ascii.clone_from(f);
        doc.style.body_font_east_asia.clone_from(f);
    }

    // Body text size
    if let Some(sz) = ovr.text_size_half_pt {
        doc.style.body_size_half_pt = sz;
    }

    // First-line indent (Typst default: 0pt)
    doc.style.first_line_indent_twips = ovr.first_line_indent_twips.unwrap_or(0);

    // Body paragraph spacing: source AST #set par(spacing: ...) if present,
    // otherwise Typst's documented default of 1.2em × body_size.
    let body_pt = f64::from(doc.style.body_size_half_pt) / 2.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let default_par_spacing = (1.2 * body_pt * 20.0).round() as u32;
    let par_spacing = ovr.par_spacing_twips.unwrap_or(default_par_spacing);
    doc.style.body_spacing_before = par_spacing;
    doc.style.body_spacing_after = par_spacing;

    // Line spacing: source AST #set par(leading: ...) if present,
    // otherwise Typst's documented default of 0.65em.
    {
        let body_pt = f64::from(doc.style.body_size_half_pt) / 2.0;
        let leading_pt = if let Some(leading_twips) = ovr.par_leading_twips {
            f64::from(leading_twips) / 20.0
        } else {
            0.65 * body_pt
        };
        let line_height_pt = body_pt + leading_pt;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let spacing = (line_height_pt / body_pt * 240.0).round() as u32;
        doc.style.line_spacing = spacing;
    }

    // Paragraph justification
    if let Some(justify) = ovr.justify {
        doc.style.body_alignment = if justify {
            "both".to_string()
        } else {
            "left".to_string()
        };
    }

    // Heading spacing: compute from Typst's documented defaults.
    // Typst heading above/below are relative to the heading's own font size:
    //   scale = [1.4, 1.2, 1.0, 1.0, 1.0] for levels 1-5
    //   above = (if level==1 { 1.8 } else { 1.44 }) / scale  (in em of heading size)
    //   below = 0.75 / scale  (in em of heading size)
    // The heading size in pt = heading_sizes[level] / 2.
    {
        let scales = [1.4_f64, 1.2, 1.0, 1.0, 1.0];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for (level, &scale) in scales.iter().enumerate() {
            let heading_pt = f64::from(doc.style.heading_sizes[level]) / 2.0;
            let above_em = if level == 0 { 1.8 } else { 1.44 } / scale;
            let below_em = 0.75 / scale;
            let above_pt = above_em * heading_pt;
            let below_pt = below_em * heading_pt;
            doc.style.heading_spacing_before[level] =
                (above_pt * 20.0).round() as u32;
            doc.style.heading_spacing_after[level] =
                (below_pt * 20.0).round() as u32;
        }
    }
}

/// Recursively walk `HtmlNode` children, dispatching on `Tag::Start` element types.
///
/// `page_breaks` contains `Location`s of elements that should have a page
/// break inserted before them (computed by [`collect_page_break_locations`]).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn walk_tags(
    children: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
    page_breaks: &HashSet<Location>,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    // Insert a page break before this element if the
                    // introspector-based pre-pass determined there is one.
                    if page_breaks.contains(&tag.location()) {
                        let mut pb_para = Paragraph::new();
                        pb_para.add_page_break();
                        doc.add_paragraph(pb_para);
                    }
                    let elem_name = content.elem().name();
                    match elem_name {
                        "heading" => {
                            handle_heading(tag, html_doc, doc, bookmarks);
                            // Track chapter changes for equation numbering
                            if let Some(c) = html_doc
                                .introspector
                                .query_first(&typst::foundations::Selector::Location(
                                    tag.location(),
                                ))
                                .and_then(|c| c.to_packed::<HeadingElem>().cloned())
                            {
                                let level = c.resolve_level(StyleChain::default()).get();
                                if level == 1 {
                                    eq_state.chapter += 1;
                                    eq_state.eq_in_chapter = 0;
                                }
                            }
                            // Skip past the heading's inner HTML elements to the matching End tag
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "par" => {
                            // Merge subsequent inline equations + par fragments
                            // into a single Word paragraph.
                            i = handle_par_with_inline_equations(
                                children,
                                i,
                                html_doc,
                                doc,
                                eq_state,
                                image_queue,
                                bookmarks,
                            );
                        }
                        "equation" => {
                            handle_equation(tag, html_doc, doc, eq_state, bookmarks);
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
                            handle_table(
                                &children[i..=end],
                                html_doc,
                                doc,
                                eq_state,
                                image_queue,
                                bookmarks,
                                page_breaks,
                            );
                            i = end;
                        }
                        "list" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(
                                &children[i..=end],
                                html_doc,
                                doc,
                                false,
                                eq_state,
                                image_queue,
                                bookmarks,
                                page_breaks,
                            );
                            i = end;
                        }
                        "enum" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(
                                &children[i..=end],
                                html_doc,
                                doc,
                                true,
                                eq_state,
                                image_queue,
                                bookmarks,
                                page_breaks,
                            );
                            i = end;
                        }
                        "image" => {
                            // Consume the next image from the queue extracted from PagedDocument
                            if !image_queue.is_empty() {
                                let img_data = image_queue.pop_front().unwrap();
                                let mut para = Paragraph::new();
                                para.add_image(img_data);
                                doc.add_paragraph(para);
                            }
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "figure" | "section" => {
                            // Recurse into inner children between Start and End
                            let end = find_tag_end(children, i, tag.location());
                            // Skip doc-endnotes sections
                            if elem_name == "section" && is_doc_endnotes_section(&children[i..=end])
                            {
                                i = end;
                                i += 1;
                                continue;
                            }
                            // For figures, insert a bookmark if the content has a label
                            if elem_name == "figure"
                                && let Some(label) = content.label()
                            {
                                let label_str = format!("{}", label.resolve());
                                if !bookmarks.contains(&label_str) {
                                    bookmarks.insert(label_str.clone());
                                    let bk_id = doc.next_bookmark_id();
                                    let mut bk_para = Paragraph::new();
                                    bk_para.add_bookmark(bk_id, label_str);
                                    doc.add_paragraph(bk_para);
                                }
                            }
                            walk_tags(
                                &children[i + 1..end],
                                html_doc,
                                doc,
                                eq_state,
                                image_queue,
                                bookmarks,
                                page_breaks,
                            );
                            i = end;
                        }
                        "outline" => {
                            let depth: u8 = html_doc
                                .introspector
                                .query_first(&typst::foundations::Selector::Location(
                                    tag.location(),
                                ))
                                .and_then(|c| c.to_packed::<OutlineElem>().cloned())
                                .and_then(|o| *o.depth.as_option())
                                .flatten()
                                .map_or(3, |d| u8::try_from(d.get()).unwrap_or(3));
                            let mut para = Paragraph::new();
                            para.add_toc(depth);
                            doc.add_paragraph(para);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "pagebreak" => {
                            let mut para = Paragraph::new();
                            para.add_page_break();
                            doc.add_paragraph(para);
                            let end = find_tag_end(children, i, tag.location());
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
                handle_html_element(
                    elem,
                    html_doc,
                    doc,
                    eq_state,
                    image_queue,
                    bookmarks,
                    page_breaks,
                );
            }
            HtmlNode::Text(text, span) => {
                // Bare text outside of any Tag — emit as a paragraph
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    let mut run = Run::new(trimmed);
                    if !span.is_detached() {
                        run.span = Some(*span);
                    }
                    para.push_run(run);
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
#[allow(clippy::too_many_arguments)]
fn handle_html_element(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
    page_breaks: &HashSet<Location>,
) {
    let tag = tag_name(elem);
    match tag.as_str() {
        "pre" => convert_code_block(elem, doc),
        "blockquote" => convert_blockquote(
            elem,
            html_doc,
            doc,
            eq_state,
            image_queue,
            bookmarks,
            page_breaks,
        ),
        "dl" => convert_term_list(elem, doc),
        "ol" => convert_html_list(elem, doc, true),
        "ul" => convert_html_list(elem, doc, false),
        "table" => convert_html_table(elem, doc, html_doc),
        "figcaption" => {
            // Collect all figcaption content into a single paragraph
            let mut para = Paragraph::new();
            para.alignment = Some(Alignment::Center);
            collect_html_inlines(&elem.children, &mut para, false, false, false);
            if !para.inlines.is_empty() {
                doc.add_paragraph(para);
            }
        }
        "section" => {
            // Skip doc-endnotes section
            if has_attr_value(elem, "role", "doc-endnotes") {
                return;
            }
            walk_tags(
                &elem.children,
                html_doc,
                doc,
                eq_state,
                image_queue,
                bookmarks,
                page_breaks,
            );
        }
        _ => {
            // Check for alignment on this element and apply to child paragraphs
            let alignment = detect_alignment(elem);
            let start_idx = doc.body.elements.len();
            walk_tags(
                &elem.children,
                html_doc,
                doc,
                eq_state,
                image_queue,
                bookmarks,
                page_breaks,
            );
            if let Some(align) = alignment {
                for element in &mut doc.body.elements[start_idx..] {
                    if let BlockElement::Paragraph(para) = element {
                        para.alignment = Some(align.clone());
                    }
                }
            }
        }
    }
}

/// Handle a `HeadingElem` tag: query the introspector for the full Content,
/// extract level + body runs, and emit a heading paragraph.
fn handle_heading(
    tag: &Tag,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    bookmarks: &mut HashSet<String>,
) {
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

    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level_u8));

    // Insert bookmark if heading has a label
    if let Some(label) = content.label() {
        let label_str = format!("{}", label.resolve());
        if !bookmarks.contains(&label_str) {
            bookmarks.insert(label_str.clone());
            let bk_id = doc.next_bookmark_id();
            para.add_bookmark(bk_id, label_str);
        }
    }

    // Walk heading body — handle both text runs and inline math
    extract_heading_content(&heading.body, &mut para);
    doc.add_paragraph(para);
}

/// Walk a heading's body content, extracting text runs and inline math.
fn extract_heading_content(content: &typst::foundations::Content, para: &mut Paragraph) {
    use typst_library::foundations::SequenceElem;

    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            extract_heading_content(child, para);
        }
    } else if content.to_packed::<EquationElem>().is_some() {
        let omml = typort_math::equation_to_omml(content);
        para.add_math(omml);
    } else {
        for run in inline::extract_runs(content) {
            para.push_run(run);
        }
    }
}

/// Handle a `par` Tag: collect inline children (text, strong, emph, equation, footnote)
/// and emit a paragraph.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn handle_par(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
) {
    let mut para = Paragraph::new();
    // Skip the first Tag::Start("par") and collect inlines from the inner nodes
    let inner = &slice[1..slice.len().saturating_sub(1)];
    collect_par_inlines(
        inner,
        html_doc,
        doc,
        &mut para,
        eq_state,
        image_queue,
        bookmarks,
    );
    if !para.inlines.is_empty() {
        doc.add_paragraph(para);
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
#[allow(clippy::too_many_arguments)]
fn handle_par_with_inline_equations(
    children: &[HtmlNode],
    par_start: usize,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
) -> usize {
    let HtmlNode::Tag(tag) = &children[par_start] else {
        return par_start;
    };
    let par_end = find_tag_end(children, par_start, tag.location());

    // Check if the next sibling after this par is an inline equation.
    // If not, just handle as a normal paragraph (fast path).
    let next_start = par_end + 1;
    if !is_inline_equation_at(children, next_start, html_doc) {
        handle_par(
            &children[par_start..=par_end],
            html_doc,
            doc,
            eq_state,
            image_queue,
            bookmarks,
        );
        return par_end;
    }

    // Merge mode: build a single paragraph from par + inline eq + par + ...
    let mut para = Paragraph::new();

    // Collect inlines from the first par fragment
    let inner = &children[par_start + 1..par_end];
    collect_par_inlines(
        inner,
        html_doc,
        doc,
        &mut para,
        eq_state,
        image_queue,
        bookmarks,
    );

    // The pattern is strictly: equation -> par -> equation -> par -> ...
    // After each inline equation, we expect a continuation par.
    // After each continuation par, we ONLY continue if the next thing is
    // another inline equation (otherwise this par is actually a new paragraph).
    let mut cursor = next_start;
    while cursor < children.len() {
        // Step 1: expect an inline equation
        if !is_inline_equation_at(children, cursor, html_doc) {
            break;
        }
        if let HtmlNode::Tag(eq_tag) = &children[cursor] {
            let loc = eq_tag.location();
            if let Some(c) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
            {
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
            collect_par_inlines(
                p_inner,
                html_doc,
                doc,
                &mut para,
                eq_state,
                image_queue,
                bookmarks,
            );
            cursor = p_end + 1;
        } else {
            break;
        }
        // Loop back: if next is another inline equation, continue merging.
        // Otherwise, the loop condition will break out.
    }

    if !para.inlines.is_empty() {
        doc.add_paragraph(para);
    }

    // Return index of last consumed node (cursor - 1 since the outer loop does i += 1)
    cursor.saturating_sub(1)
}

/// Check if position `idx` in `children` is a `Tag::Start("equation")` for an
/// **inline** (non-block) equation.
fn is_inline_equation_at(children: &[HtmlNode], idx: usize, html_doc: &HtmlDocument) -> bool {
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
    let Some(c) = html_doc
        .introspector
        .query_first(&typst::foundations::Selector::Location(loc))
    else {
        return false;
    };
    let eq_packed = c.to_packed::<EquationElem>();
    let is_block = eq_packed
        .as_ref()
        .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));
    !is_block
}

/// Check if position `idx` in `children` is a `Tag::Start("par")`.
fn is_par_tag_at(children: &[HtmlNode], idx: usize) -> bool {
    let Some(HtmlNode::Tag(tag)) = children.get(idx) else {
        return false;
    };
    let Tag::Start(content, _) = tag else {
        return false;
    };
    content.elem().name() == "par"
}

/// Collect inline elements from nodes inside a paragraph.
/// This handles Text, `Tag::Start` for strong/emph/equation/footnote, and HTML elements.
#[allow(clippy::too_many_arguments)]
fn collect_par_inlines(
    children: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Text(text, span) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    if !span.is_detached() {
                        run.span = Some(*span);
                    }
                    para.push_run(run);
                }
            }
            HtmlNode::Tag(tag) => {
                if let Tag::Start(..) = tag {
                    i = handle_inline_tag(
                        tag,
                        children,
                        i,
                        html_doc,
                        doc,
                        para,
                        eq_state,
                        image_queue,
                        bookmarks,
                    );
                }
            }
            HtmlNode::Element(elem) => {
                handle_inline_html_element(
                    elem,
                    html_doc,
                    doc,
                    para,
                    eq_state,
                    image_queue,
                    bookmarks,
                );
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

/// Process a single inline `Tag::Start` within a paragraph.
/// Returns the new index (pointing at the matching End tag).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn handle_inline_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
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
                let eq_packed = c.to_packed::<EquationElem>();
                let is_block = eq_packed
                    .as_ref()
                    .is_some_and(|eq| *eq.block.as_option().as_ref().unwrap_or(&false));
                if is_block {
                    if !para.inlines.is_empty() {
                        let prev = std::mem::take(para);
                        doc.add_paragraph(prev);
                    }
                    let eq_number = compute_equation_number(eq_packed, eq_state);
                    let mut math_para = Paragraph::new();
                    // Insert bookmark if equation has a label
                    if let Some(label) = c.label() {
                        let label_str = format!("{}", label.resolve());
                        if !bookmarks.contains(&label_str) {
                            bookmarks.insert(label_str.clone());
                            let bk_id = doc.next_bookmark_id();
                            math_para.add_bookmark(bk_id, label_str);
                        }
                    }
                    if let Some(number) = eq_number {
                        math_para.add_numbered_math(omml, number);
                    } else {
                        math_para.add_math(omml);
                    }
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
        "image" => {
            // Inline image within a paragraph
            if !image_queue.is_empty() {
                let img_data = image_queue.pop_front().unwrap();
                para.add_image(img_data);
            }
            find_tag_end(children, i, tag.location())
        }
        "ref" => {
            // Cross-reference: extract target label and display text
            let end = find_tag_end(children, i, tag.location());
            let loc = tag.location();
            if let Some(c) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                && let Some(ref_elem) = c.to_packed::<RefElem>()
            {
                let target_label = format!("{}", ref_elem.target.resolve());
                let display = collect_text_from_nodes(&children[i + 1..end]);
                para.add_field_ref(target_label, display);
            }
            end
        }
        "link" => {
            // Hyperlink: extract URL and formatted display runs
            let end = find_tag_end(children, i, tag.location());
            let loc = tag.location();
            if let Some(c) = html_doc
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                && let Some(link_elem) = c.to_packed::<typst_library::model::LinkElem>()
            {
                let url = match &link_elem.dest {
                    typst_library::model::LinkTarget::Dest(
                        typst_library::model::Destination::Url(u),
                    ) => u.to_string(),
                    _ => String::new(),
                };
                if url.is_empty() {
                    return end;
                }
                // Collect formatted runs from link children, preserving bold/italic/etc.
                let runs = collect_formatted_runs_from_nodes(&children[i + 1..end]);
                if !runs.is_empty() {
                    para.add_hyperlink(url, runs);
                }
            }
            end
        }
        "pagebreak" => {
            para.add_page_break();
            find_tag_end(children, i, tag.location())
        }
        "super" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.superscript = true;
                para.push_run(run);
            }
            end
        }
        "sub" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.subscript = true;
                para.push_run(run);
            }
            end
        }
        "raw" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.monospace = true;
                para.push_run(run);
            }
            end
        }
        "underline" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.underline = true;
                para.push_run(run);
            }
            end
        }
        "strike" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.strikethrough = true;
                para.push_run(run);
            }
            end
        }
        "highlight" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.highlight = true;
                para.push_run(run);
            }
            end
        }
        "overline" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.underline = true; // Word has no overline; underline is the closest
                para.push_run(run);
            }
            end
        }
        "smallcaps" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_text_from_nodes(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                run.smallcaps = true;
                para.push_run(run);
            }
            end
        }
        _ => {
            // Skip unknown or non-inline tags
            find_tag_end(children, i, tag.location())
        }
    }
}

/// Process a single inline HTML element within a paragraph.
#[allow(clippy::too_many_arguments)]
fn handle_inline_html_element(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    para: &mut Paragraph,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
) {
    let tag_str = tag_name(elem);
    match tag_str.as_str() {
        "strong" | "b" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, true, false, false);
            for run in drain_text_runs(&mut tmp) {
                para.push_run(run);
            }
        }
        "em" | "i" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, true, false);
            for run in drain_text_runs(&mut tmp) {
                para.push_run(run);
            }
        }
        "code" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, false, true);
            for run in drain_text_runs(&mut tmp) {
                para.push_run(run);
            }
        }
        "a" if has_attr_value(elem, "role", "doc-noteref") => {
            // Already handled by Tag::Start("footnote")
        }
        "a" => {
            // External hyperlink from HTML <a href="...">
            if let Some(href) = get_attr_value(elem, "href") {
                let mut tmp = Paragraph::new();
                collect_html_inlines(&elem.children, &mut tmp, false, false, false);
                let runs = drain_text_runs(&mut tmp);
                if !runs.is_empty() {
                    para.add_hyperlink(href, runs);
                }
            } else {
                collect_par_inlines(
                    &elem.children,
                    html_doc,
                    doc,
                    para,
                    eq_state,
                    image_queue,
                    bookmarks,
                );
            }
        }
        "sup" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, false, false);
            for mut run in drain_text_runs(&mut tmp) {
                run.superscript = true;
                para.push_run(run);
            }
        }
        "sub" => {
            let mut tmp = Paragraph::new();
            collect_html_inlines(&elem.children, &mut tmp, false, false, false);
            for mut run in drain_text_runs(&mut tmp) {
                run.subscript = true;
                para.push_run(run);
            }
        }
        _ => {
            collect_par_inlines(
                &elem.children,
                html_doc,
                doc,
                para,
                eq_state,
                image_queue,
                bookmarks,
            );
        }
    }
}

/// Collect inline elements from HTML nodes (used for table cells, list items, etc.)
///
/// This variant does not resolve inline equations. For table cells that may
/// contain math, use the `_with_doc` variant instead.
fn collect_html_inlines(
    children: &[HtmlNode],
    para: &mut Paragraph,
    bold: bool,
    italic: bool,
    monospace: bool,
) {
    collect_html_inlines_with_doc(children, para, bold, italic, monospace, None);
}

/// Inner implementation of `collect_html_inlines` that optionally accepts an
/// `HtmlDocument` for resolving inline equations via the Introspector.
fn collect_html_inlines_with_doc(
    children: &[HtmlNode],
    para: &mut Paragraph,
    bold: bool,
    italic: bool,
    monospace: bool,
    html_doc: Option<&HtmlDocument>,
) {
    for child in children {
        match child {
            HtmlNode::Text(text, span) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    run.italic = italic;
                    run.monospace = monospace;
                    if !span.is_detached() {
                        run.span = Some(*span);
                    }
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
                collect_html_inlines_with_doc(
                    &elem.children,
                    para,
                    new_bold,
                    new_italic,
                    new_monospace,
                    html_doc,
                );
            }
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    let elem_name = content.elem().name();
                    if elem_name == "footnote" {
                        if let Some(id) = find_footnote_id_in_range(
                            &children[children
                                .iter()
                                .position(|c| std::ptr::eq(c, child))
                                .unwrap_or(0)..],
                        ) {
                            para.add_footnote_ref(id + 1);
                        }
                    } else if elem_name == "equation"
                        && let Some(doc) = html_doc
                    {
                        let loc = tag.location();
                        if let Some(c) = doc
                            .introspector
                            .query_first(&typst::foundations::Selector::Location(loc))
                        {
                            let omml = typort_math::equation_to_omml(&c);
                            para.add_math(omml);
                        }
                    }
                }
            }
            HtmlNode::Frame(_) => {}
        }
    }
}

/// Handle a block-level equation Tag.
fn handle_equation(
    tag: &Tag,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    bookmarks: &mut HashSet<String>,
) {
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
        // Insert bookmark if equation has a label
        if let Some(label) = content.label() {
            let label_str = format!("{}", label.resolve());
            if !bookmarks.contains(&label_str) {
                bookmarks.insert(label_str.clone());
                let bk_id = doc.next_bookmark_id();
                para.add_bookmark(bk_id, label_str);
            }
        }
        let eq_number = compute_equation_number(eq_packed, eq_state);
        if let Some(number) = eq_number {
            para.add_numbered_math(omml, number);
        } else {
            para.add_math(omml);
        }
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
#[allow(clippy::too_many_arguments)]
fn handle_table(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
    page_breaks: &HashSet<Location>,
) {
    // Look for an HTML <table> element within the tag range
    for node in slice {
        if let HtmlNode::Element(elem) = node {
            let tag = tag_name(elem);
            if tag == "table" {
                convert_html_table(elem, doc, html_doc);
                return;
            }
            // Recurse into child elements to find the table
            if find_and_convert_table_in_elem(elem, doc, html_doc) {
                return;
            }
        }
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(
        inner,
        html_doc,
        doc,
        eq_state,
        image_queue,
        bookmarks,
        page_breaks,
    );
}

/// Recursively search for a `<table>` element within an HTML element tree.
fn find_and_convert_table_in_elem(
    elem: &HtmlElement,
    doc: &mut Document,
    html_doc: &HtmlDocument,
) -> bool {
    for child in &elem.children {
        if let HtmlNode::Element(inner) = child {
            let tag = tag_name(inner);
            if tag == "table" {
                convert_html_table(inner, doc, html_doc);
                return true;
            }
            if find_and_convert_table_in_elem(inner, doc, html_doc) {
                return true;
            }
        }
    }
    false
}

/// Handle a `list` or `enum` Tag: find the HTML `<ul>` or `<ol>` element in the inner
/// children and parse it.
#[allow(clippy::too_many_arguments)]
fn handle_list(
    slice: &[HtmlNode],
    html_doc: &HtmlDocument,
    doc: &mut Document,
    ordered: bool,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
    page_breaks: &HashSet<Location>,
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
    walk_tags(
        inner,
        html_doc,
        doc,
        eq_state,
        image_queue,
        bookmarks,
        page_breaks,
    );
}

/// Recursively search for a `<ul>` or `<ol>` element.
fn find_and_convert_list_in_elem(elem: &HtmlElement, doc: &mut Document, ordered: bool) -> bool {
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
fn convert_html_table(elem: &HtmlElement, doc: &mut Document, html_doc: &HtmlDocument) {
    let mut raw_rows: Vec<RawTableRow> = Vec::new();
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                if let Some(result) = convert_table_row(row_or_section, html_doc) {
                    raw_rows.push(result);
                }
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                        && let Some(result) = convert_table_row(tr, html_doc)
                    {
                        raw_rows.push(result);
                    }
                }
            }
        }
    }

    if raw_rows.is_empty() {
        return;
    }

    // Post-process: insert vMerge::Continue cells where rowspans require them
    let table = postprocess_rowspans(raw_rows);
    doc.add_table(table);
}

/// Post-process table rows to insert `VMerge::Continue` cells for rowspans.
///
/// In HTML, when a cell has `rowspan=N`, the subsequent N-1 rows omit the cell at that
/// column position. In OOXML, every row must have the same number of logical columns,
/// and continuation cells must have `<w:vMerge/>` (no val = continue).
fn postprocess_rowspans(raw_rows: Vec<RawTableRow>) -> Table {
    // Track active rowspans: (logical_col_index, rows_remaining, colspan)
    // `rows_remaining` counts how many MORE rows need a continuation cell.
    let mut active_spans: Vec<(usize, u32, u32)> = Vec::new();
    let mut final_rows = Vec::new();

    for (row, span_info) in raw_rows {
        // Sort active spans by column index
        active_spans.sort_by_key(|(col, _, _)| *col);

        // Build the new row by interleaving continuation cells with source cells
        let mut new_cells = Vec::new();
        let mut logical_col: usize = 0;
        let mut src_idx: usize = 0;
        let src_cells = row.cells;

        loop {
            // Check if this logical column needs a continuation cell
            if let Some(&(_, _, colspan)) = active_spans.iter().find(|(c, _, _)| *c == logical_col)
            {
                new_cells.push(TableCell {
                    paragraphs: vec![Paragraph::new()],
                    content: Vec::new(),
                    colspan,
                    vmerge: VMerge::Continue,
                    width_pct: None,
                });
                logical_col += colspan as usize;
            } else if src_idx < src_cells.len() {
                logical_col += src_cells[src_idx].colspan as usize;
                new_cells.push(src_cells[src_idx].clone());
                src_idx += 1;
            } else {
                break;
            }
        }

        // Decrement active spans and remove expired ones (AFTER using them for this row)
        active_spans.retain_mut(|(_, remaining, _)| {
            *remaining -= 1;
            *remaining > 0
        });

        // Register new rowspans from this row's span_info.
        // span_info indices are relative to the HTML source cells; remap to logical positions.
        for (html_col_idx, rowspan, colspan) in &span_info {
            // Find the logical column for this HTML cell index by walking the final cells,
            // skipping continuation cells (which don't correspond to HTML source cells).
            let mut logical = 0_usize;
            let mut html_idx = 0_usize;
            for cell in &new_cells {
                if cell.vmerge == VMerge::Continue {
                    logical += cell.colspan as usize;
                    continue;
                }
                if html_idx == *html_col_idx {
                    break;
                }
                logical += cell.colspan as usize;
                html_idx += 1;
            }
            if *rowspan > 1 {
                active_spans.push((logical, *rowspan - 1, *colspan));
            }
        }

        final_rows.push(TableRow { cells: new_cells });
    }

    Table { rows: final_rows }
}

/// Convert a `<tr>` element into a `TableRow` plus rowspan metadata.
///
/// Returns `(TableRow, Vec<(cell_index, rowspan, colspan)>)` where the second
/// element records which cells have `rowspan > 1` so the caller can insert
/// `VMerge::Continue` cells in subsequent rows.
fn convert_table_row(tr: &HtmlElement, html_doc: &HtmlDocument) -> Option<RawTableRow> {
    let mut cells = Vec::new();
    let mut span_info = Vec::new();
    let mut cell_idx: usize = 0;

    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                let is_header = tag == "th";
                // Check if <td> children include <p> elements for multi-paragraph cells
                let paragraphs = convert_cell_paragraphs(td, is_header, html_doc);

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

                if rowspan > 1 {
                    span_info.push((cell_idx, rowspan, colspan));
                }

                // Check for nested tables within the cell
                let (final_paragraphs, cell_content) =
                    extract_cell_content_with_nested_tables(td, is_header, html_doc, paragraphs);

                cells.push(TableCell {
                    paragraphs: final_paragraphs,
                    content: cell_content,
                    colspan,
                    vmerge,
                    width_pct: None,
                });
                cell_idx += 1;
            }
        }
    }
    if cells.is_empty() {
        None
    } else {
        Some((TableRow { cells }, span_info))
    }
}

/// Convert a `<td>` or `<th>` element's children into paragraphs.
///
/// If the cell contains `<p>` child elements, each `<p>` becomes a separate paragraph.
/// Otherwise, all inline content is collected into a single paragraph.
fn convert_cell_paragraphs(
    td: &HtmlElement,
    is_header: bool,
    html_doc: &HtmlDocument,
) -> Vec<Paragraph> {
    // Check if any direct children are <p> elements
    let has_p_children = td.children.iter().any(|c| {
        if let HtmlNode::Element(el) = c {
            tag_name(el) == "p"
        } else {
            false
        }
    });

    if has_p_children {
        let mut paragraphs = Vec::new();
        for child in &td.children {
            if let HtmlNode::Element(el) = child
                && tag_name(el) == "p"
            {
                let mut para = Paragraph::new();
                collect_html_inlines_with_doc(
                    &el.children,
                    &mut para,
                    is_header,
                    false,
                    false,
                    Some(html_doc),
                );
                if !para.inlines.is_empty() {
                    paragraphs.push(para);
                }
            }
        }
        if paragraphs.is_empty() {
            // Fallback: collect all content as one paragraph
            let mut para = Paragraph::new();
            collect_html_inlines_with_doc(
                &td.children,
                &mut para,
                is_header,
                false,
                false,
                Some(html_doc),
            );
            vec![para]
        } else {
            paragraphs
        }
    } else {
        let mut para = Paragraph::new();
        collect_html_inlines_with_doc(
            &td.children,
            &mut para,
            is_header,
            false,
            false,
            Some(html_doc),
        );
        vec![para]
    }
}

/// Check if a `<td>`/`<th>` element contains nested `<table>` elements and,
/// if so, build a `Vec<CellContent>` that interleaves paragraphs and nested
/// tables in document order.
///
/// Returns `(paragraphs, content)` where:
/// - `paragraphs` is the original paragraph list (kept for backward compat)
/// - `content` is non-empty only when nested tables are present
fn extract_cell_content_with_nested_tables(
    td: &HtmlElement,
    _is_header: bool,
    html_doc: &HtmlDocument,
    paragraphs: Vec<Paragraph>,
) -> (Vec<Paragraph>, Vec<CellContent>) {
    // Check if any child (direct or nested in a wrapper div/span) is a <table>
    let has_nested_table = has_table_descendant(td);
    if !has_nested_table {
        return (paragraphs, Vec::new());
    }

    // Walk children in order, collecting paragraphs and nested tables
    let mut content: Vec<CellContent> = Vec::new();
    collect_cell_content_recursive(&td.children, html_doc, &mut content);

    // Also build the flat paragraphs list for backward compat
    let flat_paragraphs: Vec<Paragraph> = content
        .iter()
        .filter_map(|c| {
            if let CellContent::Paragraph(p) = c {
                Some(p.clone())
            } else {
                None
            }
        })
        .collect();

    let final_paragraphs = if flat_paragraphs.is_empty() {
        vec![Paragraph::new()]
    } else {
        flat_paragraphs
    };

    (final_paragraphs, content)
}

/// Check if an element has any `<table>` descendant.
fn has_table_descendant(elem: &HtmlElement) -> bool {
    for child in &elem.children {
        if let HtmlNode::Element(el) = child {
            if tag_name(el) == "table" {
                return true;
            }
            if has_table_descendant(el) {
                return true;
            }
        }
    }
    false
}

/// Recursively collect cell content (paragraphs and nested tables) from HTML
/// children, preserving document order.
fn collect_cell_content_recursive(
    children: &[HtmlNode],
    html_doc: &HtmlDocument,
    content: &mut Vec<CellContent>,
) {
    for child in children {
        match child {
            HtmlNode::Element(el) => {
                let tag = tag_name(el);
                if tag == "table" {
                    // Convert this as a nested table
                    let table = convert_html_table_to_model(el, html_doc);
                    if let Some(t) = table {
                        content.push(CellContent::Table(t));
                    }
                } else if tag == "p" {
                    let mut para = Paragraph::new();
                    collect_html_inlines_with_doc(
                        &el.children,
                        &mut para,
                        false,
                        false,
                        false,
                        Some(html_doc),
                    );
                    if !para.inlines.is_empty() {
                        content.push(CellContent::Paragraph(para));
                    }
                } else {
                    // Recurse into wrapper elements (div, span, etc.)
                    collect_cell_content_recursive(&el.children, html_doc, content);
                }
            }
            HtmlNode::Text(text, _) => {
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.add_run(trimmed);
                    content.push(CellContent::Paragraph(para));
                }
            }
            _ => {}
        }
    }
}

/// Convert an HTML `<table>` element into a `Table` model (without adding to doc).
/// Returns `None` if the table has no rows.
fn convert_html_table_to_model(elem: &HtmlElement, html_doc: &HtmlDocument) -> Option<Table> {
    let mut raw_rows: Vec<RawTableRow> = Vec::new();
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                if let Some(result) = convert_table_row(row_or_section, html_doc) {
                    raw_rows.push(result);
                }
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                        && let Some(result) = convert_table_row(tr, html_doc)
                    {
                        raw_rows.push(result);
                    }
                }
            }
        }
    }

    if raw_rows.is_empty() {
        None
    } else {
        Some(postprocess_rowspans(raw_rows))
    }
}

/// Convert an HTML `<ol>` or `<ul>` element into list paragraphs.
fn convert_html_list(elem: &HtmlElement, doc: &mut Document, ordered: bool) {
    convert_html_list_at_level(elem, doc, ordered, 0);
}

fn convert_html_list_at_level(elem: &HtmlElement, doc: &mut Document, ordered: bool, level: u32) {
    let list_id = if ordered { 1 } else { 2 };
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            para.list_id = Some(list_id);
            para.list_level = Some(level);
            // Collect only direct inline content, skipping nested sub-lists
            let non_list_children: Vec<&HtmlNode> = li
                .children
                .iter()
                .filter(|c| {
                    if let HtmlNode::Element(el) = c {
                        let t = tag_name(el);
                        t != "ul" && t != "ol"
                    } else {
                        true
                    }
                })
                .collect();
            for c in &non_list_children {
                match c {
                    HtmlNode::Text(text, _) if !text.is_empty() => {
                        para.push_run(Run::new(text.as_str()));
                    }
                    HtmlNode::Element(el) => {
                        collect_html_inlines(
                            &el.children,
                            &mut para,
                            tag_name(el) == "strong" || tag_name(el) == "b",
                            tag_name(el) == "em" || tag_name(el) == "i",
                            tag_name(el) == "code",
                        );
                    }
                    _ => {}
                }
            }
            if !para.inlines.is_empty() {
                doc.add_paragraph(para);
            }
            for li_child in &li.children {
                if let HtmlNode::Element(sub) = li_child {
                    let sub_tag = tag_name(sub);
                    if sub_tag == "ul" {
                        convert_html_list_at_level(sub, doc, false, level + 1);
                    } else if sub_tag == "ol" {
                        convert_html_list_at_level(sub, doc, true, level + 1);
                    }
                }
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
#[allow(clippy::too_many_arguments)]
fn convert_blockquote(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
    doc: &mut Document,
    eq_state: &mut EquationState,
    image_queue: &mut VecDeque<ImageData>,
    bookmarks: &mut HashSet<String>,
    page_breaks: &HashSet<Location>,
) {
    let start_idx = doc.body.elements.len();
    walk_tags(
        &elem.children,
        html_doc,
        doc,
        eq_state,
        image_queue,
        bookmarks,
        page_breaks,
    );
    // Apply left indent to all paragraphs added by the blockquote
    for element in &mut doc.body.elements[start_idx..] {
        if let BlockElement::Paragraph(para) = element {
            para.left_indent = Some(doc.style.first_line_indent_twips);
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
                    if !para.inlines.is_empty() {
                        doc.add_paragraph(para);
                    }
                }
                "dd" => {
                    let mut para = Paragraph::new();
                    para.left_indent = Some(doc.style.first_line_indent_twips);
                    para.suppress_indent = true;
                    collect_html_inlines(&item.children, &mut para, false, false, false);
                    if !para.inlines.is_empty() {
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

/// Compute the equation number string for a block equation, if it has numbering.
fn compute_equation_number(
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
        Some(pattern.apply(&nums).to_string())
    } else {
        None
    }
}

/// Collect all text content from a slice of `HtmlNode` (used for cross-reference display text).
fn collect_text_from_nodes(nodes: &[HtmlNode]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(t, _) => text.push_str(t),
            HtmlNode::Element(elem) => text.push_str(&collect_all_text(&elem.children)),
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
    text
}

/// Collect formatted runs from a slice of `HtmlNode`, preserving bold/italic/monospace.
///
/// Used for hyperlink display text where inner formatting (e.g. `*Bold link*`) must
/// be preserved on the `Run` objects.
fn collect_formatted_runs_from_nodes(nodes: &[HtmlNode]) -> Vec<Run> {
    let mut para = Paragraph::new();
    // Walk through Tag nodes to handle strong/emph within the link's content
    collect_formatted_runs_inner(nodes, &mut para, false, false, false);
    drain_text_runs(&mut para)
}

/// Inner helper for collecting formatted runs, tracking inherited formatting state.
fn collect_formatted_runs_inner(
    nodes: &[HtmlNode],
    para: &mut Paragraph,
    bold: bool,
    italic: bool,
    monospace: bool,
) {
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            HtmlNode::Text(text, span) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    run.italic = italic;
                    run.monospace = monospace;
                    if !span.is_detached() {
                        run.span = Some(*span);
                    }
                    para.push_run(run);
                }
            }
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    let elem_name = content.elem().name();
                    let end = find_tag_end(nodes, i, tag.location());
                    match elem_name {
                        "strong" => {
                            collect_formatted_runs_inner(
                                &nodes[i + 1..end],
                                para,
                                true,
                                italic,
                                monospace,
                            );
                        }
                        "emph" => {
                            collect_formatted_runs_inner(
                                &nodes[i + 1..end],
                                para,
                                bold,
                                true,
                                monospace,
                            );
                        }
                        "raw" => {
                            collect_formatted_runs_inner(
                                &nodes[i + 1..end],
                                para,
                                bold,
                                italic,
                                true,
                            );
                        }
                        _ => {
                            collect_formatted_runs_inner(
                                &nodes[i + 1..end],
                                para,
                                bold,
                                italic,
                                monospace,
                            );
                        }
                    }
                    i = end;
                }
            }
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                let new_bold = bold || tag == "strong" || tag == "b";
                let new_italic = italic || tag == "em" || tag == "i";
                let new_monospace = monospace || tag == "code";
                collect_formatted_runs_inner(
                    &elem.children,
                    para,
                    new_bold,
                    new_italic,
                    new_monospace,
                );
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Find the index of the `Tag::End` matching the given start location.
fn find_tag_end(
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

/// Post-processing: suppress first-line indent on the first paragraph after
/// each heading, and apply hanging indent to bibliography paragraphs.
fn apply_paragraph_formatting(doc: &mut Document) {
    let mut after_heading = false;
    let mut in_bibliography = false;
    let mut is_first_element = true;

    for element in &mut doc.body.elements {
        if let BlockElement::Paragraph(p) = element {
            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                // Suppress above-spacing on the first heading (Typst collapses
                // block(above) with page margin at page start).
                if is_first_element {
                    p.spacing_before = Some(0);
                }
                // Detect bibliography section
                let text = p.text_content();
                let text_lower = text.to_lowercase();
                if text.contains("参考文献")
                    || text_lower.contains("references")
                    || text_lower.contains("bibliography")
                {
                    in_bibliography = true;
                }
                after_heading = true;
            } else {
                // Normal paragraph
                if after_heading {
                    p.suppress_indent = true;
                    after_heading = false;
                }
                if in_bibliography {
                    p.hanging_indent = true;
                }
            }
            is_first_element = false;
        }
    }
}

/// Set the document title from the first heading's text.
/// Extract document metadata (title, author) from `#set document(...)` if present,
/// falling back to the first heading text for the title.
fn apply_smallcaps_from_source(world: &TyportWorld, doc: &mut Document) {
    // SmallcapsElem is consumed during Typst realization — it doesn't survive
    // in the compiled Content AST. Detect it by walking the source AST for
    // function calls to `smallcaps` (or aliases defined via `#let sc = smallcaps`).
    let source_text = world.main_source().text();
    let sc_texts = extract_smallcaps_texts_from_ast(source_text);

    if sc_texts.is_empty() {
        return;
    }

    for element in &mut doc.body.elements {
        if let BlockElement::Paragraph(p) = element {
            for inline in &mut p.inlines {
                if let InlineElement::Text(run) = inline {
                    let trimmed = run.text.trim();
                    if sc_texts
                        .iter()
                        .any(|t| trimmed == *t || t.contains(trimmed))
                    {
                        run.smallcaps = true;
                    }
                }
            }
        }
    }
}

/// Extract text content from all `smallcaps` function calls in the source AST.
///
/// Uses `typst_syntax::parse` to walk the AST, which correctly handles:
/// - Direct calls: `#smallcaps[Hello]`
/// - Aliases: `#let sc = smallcaps; #sc[Hello]`
/// - Nested content: `#smallcaps[*bold* and _italic_]`
fn extract_smallcaps_texts_from_ast(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);

    // First pass: find aliases for `smallcaps` (e.g., `#let sc = smallcaps`).
    let mut aliases: HashSet<String> = HashSet::new();
    aliases.insert("smallcaps".to_string());
    collect_smallcaps_aliases(&root, &mut aliases);

    // Second pass: find all function calls to smallcaps or its aliases,
    // and extract the text content from their content block arguments.
    let mut sc_texts = Vec::new();
    collect_smallcaps_call_texts(&root, &aliases, &mut sc_texts);
    sc_texts
}

/// Recursively find `#let X = smallcaps` bindings and add X to the alias set.
fn collect_smallcaps_aliases(node: &typst_syntax::SyntaxNode, aliases: &mut HashSet<String>) {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::LetBinding
        && let Some(binding) = node.cast::<typst_syntax::ast::LetBinding<'_>>()
    {
        // Check if the init expression is an identifier that is `smallcaps` or an alias
        if let Some(typst_syntax::ast::Expr::Ident(init_ident)) = binding.init()
            && aliases.contains(init_ident.as_str())
        {
            // The binding names are the new aliases
            for ident in binding.kind().bindings() {
                aliases.insert(ident.as_str().to_string());
            }
        }
    }
    for child in node.children() {
        collect_smallcaps_aliases(child, aliases);
    }
}

/// Recursively find function calls to smallcaps (or aliases) and collect their text content.
fn collect_smallcaps_call_texts(
    node: &typst_syntax::SyntaxNode,
    aliases: &HashSet<String>,
    texts: &mut Vec<String>,
) {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::FuncCall
        && let Some(call) = node.cast::<typst_syntax::ast::FuncCall<'_>>()
    {
        // Check if the callee is a smallcaps function or alias
        let is_smallcaps = match call.callee() {
            typst_syntax::ast::Expr::Ident(ident) => aliases.contains(ident.as_str()),
            _ => false,
        };
        if is_smallcaps {
            // Extract text from the content block argument
            for arg in call.args().items() {
                if let typst_syntax::ast::Arg::Pos(typst_syntax::ast::Expr::ContentBlock(block)) =
                    arg
                {
                    let text = collect_markup_text(block.body());
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        texts.push(trimmed);
                    }
                }
            }
        }
    }
    for child in node.children() {
        collect_smallcaps_call_texts(child, aliases, texts);
    }
}

/// Extract plain text content from a Markup AST node, stripping formatting.
fn collect_markup_text(markup: typst_syntax::ast::Markup<'_>) -> String {
    use typst_syntax::ast::{AstNode, Expr};

    let mut result = String::new();
    for expr in markup.exprs() {
        match expr {
            Expr::Text(t) => result.push_str(t.get().as_str()),
            Expr::Space(_) => result.push(' '),
            Expr::Strong(s) => {
                let inner = collect_markup_text(s.body());
                result.push_str(&inner);
            }
            Expr::Emph(e) => {
                let inner = collect_markup_text(e.body());
                result.push_str(&inner);
            }
            _ => {
                // For other expression types, try to extract text from children
                let node = expr.to_untyped();
                result.push_str(&collect_text_from_syntax_node(node));
            }
        }
    }
    result
}

/// Recursively extract all text leaf content from a syntax node.
fn collect_text_from_syntax_node(node: &typst_syntax::SyntaxNode) -> String {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::Text || node.kind() == SyntaxKind::Space {
        return node.text().to_string();
    }
    let mut result = String::new();
    for child in node.children() {
        result.push_str(&collect_text_from_syntax_node(child));
    }
    result
}

fn extract_document_metadata(html_doc: &HtmlDocument, doc: &mut Document) {
    // Prefer explicit metadata from `#set document(title: ..., author: ...)`
    if let Some(title) = &html_doc.info.title {
        doc.metadata.title = Some(title.to_string());
    }
    if !html_doc.info.author.is_empty() {
        doc.metadata.author = Some(
            html_doc
                .info
                .author
                .iter()
                .map(typst::ecow::EcoString::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    // Fall back to first heading for title if not set via `#set document(title: ...)`
    if doc.metadata.title.is_none() {
        for elem in &doc.body.elements {
            if let BlockElement::Paragraph(p) = elem
                && matches!(p.style, Some(ParagraphStyle::Heading(_)))
            {
                let title = p.text_content();
                if !title.is_empty() {
                    doc.metadata.title = Some(title);
                }
                break;
            }
        }
    }
}

/// Get the tag name of an HTML element.
fn tag_name(elem: &HtmlElement) -> String {
    elem.tag.resolve().as_str().to_string()
}

/// Drain all `InlineElement::Text` runs from a paragraph, consuming them.
fn drain_text_runs(para: &mut Paragraph) -> Vec<Run> {
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
        if k.resolve().as_str() == attr_name {
            return Some(v.to_string());
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

/// Collect text from footnote <li> content, preserving inline formatting and
/// skipping the backlink anchor.
fn collect_footnote_text(children: &[HtmlNode], runs: &mut Vec<Run>) {
    collect_footnote_text_formatted(children, runs, false, false, false);
}

/// Inner helper that tracks inherited bold/italic/monospace state.
fn collect_footnote_text_formatted(
    children: &[HtmlNode],
    runs: &mut Vec<Run>,
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
                    runs.push(run);
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
                collect_footnote_text_formatted(
                    &elem.children,
                    runs,
                    new_bold,
                    new_italic,
                    new_monospace,
                );
            }
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Alignment detection (I5)
// ---------------------------------------------------------------------------

/// Detect paragraph alignment from an HTML element's `style` attribute.
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

// ---------------------------------------------------------------------------
// Footnote format detection (I3)
// ---------------------------------------------------------------------------

/// Detect whether footnotes use circled number format (①②③) by scanning the
/// HTML DOM for `<a role="doc-noteref">` containing `<sup>` with circled numbers.
fn detect_footnote_format(children: &[HtmlNode], doc: &mut Document) {
    for child in children {
        if let HtmlNode::Element(elem) = child {
            if has_attr_value(elem, "role", "doc-noteref")
                && let Some(text) = get_text_content(&elem.children)
                && parse_circled_number(text.trim()).is_some()
            {
                doc.style.footnote_format = FootnoteFormat::CircledNumber;
                return;
            }
            for inner in &elem.children {
                if let HtmlNode::Element(sup) = inner
                    && tag_name(sup) == "sup"
                    && let Some(text) = get_text_content(&sup.children)
                    && parse_circled_number(text.trim()).is_some()
                {
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

// ---------------------------------------------------------------------------
// Image extraction from PagedDocument
// ---------------------------------------------------------------------------

/// Extract all images from a `PagedDocument` in document order.
///
/// Walks all page frames and converts `FrameItem::Image` into `ImageData`
/// with EMU dimensions based on the rendered size (not pixel size).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_images_from_paged(paged: &PagedDocument) -> Vec<ImageData> {
    let mut images = Vec::new();
    for page in &paged.pages {
        collect_frame_images(&page.frame, &mut images);
    }
    images
}

/// Recursively collect images from a frame.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_frame_images(frame: &Frame, images: &mut Vec<ImageData>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Image(img, size, _) => {
                if let Some(data) = convert_typst_image(img, size) {
                    images.push(data);
                }
            }
            FrameItem::Group(group) => {
                collect_frame_images(&group.frame, images);
            }
            _ => {}
        }
    }
}

/// Convert a Typst Image + rendered Size into an `ImageData`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn convert_typst_image(
    img: &typst::visualize::Image,
    size: &typst::layout::Size,
) -> Option<ImageData> {
    use typst_library::visualize::ImageKind;

    // Use the rendered size for EMU dimensions: 1 pt = 12700 EMU
    let width_emu = (size.x.to_pt() * 12700.0) as u64;
    let height_emu = (size.y.to_pt() * 12700.0) as u64;

    match img.kind() {
        ImageKind::Raster(raster) => {
            use typst_library::visualize::ExchangeFormat;
            let bytes = raster.data().to_vec();
            let format = match raster.format() {
                typst_library::visualize::RasterFormat::Exchange(ExchangeFormat::Png) => {
                    ImageFormat::Png
                }
                typst_library::visualize::RasterFormat::Exchange(ExchangeFormat::Jpg) => {
                    ImageFormat::Jpeg
                }
                typst_library::visualize::RasterFormat::Exchange(
                    ExchangeFormat::Gif | ExchangeFormat::Webp,
                )
                | typst_library::visualize::RasterFormat::Pixel(_) => return None,
            };
            Some(ImageData {
                bytes,
                format,
                width_emu,
                height_emu,
            })
        }
        ImageKind::Svg(svg) => {
            let tree = svg.tree();
            let svg_size = tree.size();
            let pixel_w = svg_size.width().ceil() as u32;
            let pixel_h = svg_size.height().ceil() as u32;
            if pixel_w == 0 || pixel_h == 0 {
                return None;
            }
            let mut pixmap = tiny_skia::Pixmap::new(pixel_w, pixel_h)?;
            resvg::render(tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
            let png_bytes = pixmap.encode_png().ok()?;

            Some(ImageData {
                bytes: png_bytes,
                format: ImageFormat::Png,
                width_emu,
                height_emu,
            })
        }
        ImageKind::Pdf(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Content recovery from PagedDocument (I2)
// ---------------------------------------------------------------------------

/// A text line extracted from a `PagedDocument` frame, preserving run-level info.
#[derive(Debug, Clone)]
struct FrameLine {
    text: String,
    runs: Vec<Run>,
    /// If this line has multiple x-clusters (e.g., from a grid layout),
    /// each cluster is stored as a group of runs.  When `x_clusters` has
    /// 2+ entries the line should be emitted as a tab-separated paragraph.
    x_clusters: Vec<XCluster>,
}

/// A group of runs that belong to the same horizontal cluster on a line.
#[derive(Debug, Clone)]
struct XCluster {
    /// Average x position of this cluster (in pt).
    x_pt: f64,
    runs: Vec<Run>,
}

/// A text fragment from a rendered frame with position and size info.
struct FrameTextItem {
    y: f64,
    x: f64,
    text: String,
    size_pt: f64,
}

/// Recover content that exists in the `PagedDocument` but was lost from the `HtmlDocument` DOM.
///
/// This handles the case where `#align(center)[...]` content completely vanishes from the
/// HTML semantic output (no show rule exists for `AlignElem` in Typst's HTML target). We detect
/// the missing text by comparing the paged output with our converted document, then insert
/// the missing lines as centered paragraphs at the appropriate position.
fn recover_missing_content(paged: &PagedDocument, doc: &mut Document) {
    let all_page_lines = extract_lines_from_all_pages(paged);
    if all_page_lines.is_empty() {
        return;
    }

    let title_line_count = count_title_lines(&all_page_lines, doc);
    let body_start_line = find_body_start_line(&all_page_lines, doc);
    let full_doc_text = extract_doc_text(doc);

    // Collect header/footer text to exclude from recovery
    let header_footer_text = extract_header_footer_text(doc);

    let mut missing = Vec::new();
    for (i, line) in all_page_lines.iter().enumerate() {
        if i < title_line_count || i >= body_start_line {
            continue;
        }
        if line.text.chars().count() < 2 {
            continue;
        }
        // Skip text that's already in the body or in headers/footers
        if !full_doc_text.contains(&line.text) && !header_footer_text.contains(&line.text) {
            missing.push(line.clone());
        }
    }

    if !missing.is_empty() {
        insert_missing_at_position(doc, &missing);
    }
}

/// Extract all text from the document's header and footer for deduplication.
fn extract_header_footer_text(doc: &Document) -> String {
    let mut text = String::new();
    if let Some(header) = &doc.header {
        for para in &header.paragraphs {
            text.push_str(&para.text_content());
        }
    }
    if let Some(footer) = &doc.footer {
        for para in &footer.paragraphs {
            text.push_str(&para.text_content());
        }
    }
    text
}

/// Cluster text items by x-position.
///
/// Items within `gap_threshold` pt of each other are considered part of the
/// same cluster. Returns clusters sorted by x-position.
fn cluster_by_x<'a>(
    items: &[&'a FrameTextItem],
    gap_threshold: f64,
) -> Vec<Vec<&'a FrameTextItem>> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<&FrameTextItem> = items.to_vec();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    let mut clusters: Vec<Vec<&FrameTextItem>> = Vec::new();
    let mut current: Vec<&FrameTextItem> = vec![sorted[0]];

    for item in &sorted[1..] {
        let last_x = current.last().map_or(0.0, |i| i.x);
        if item.x - last_x > gap_threshold {
            clusters.push(std::mem::take(&mut current));
        }
        current.push(item);
    }
    if !current.is_empty() {
        clusters.push(current);
    }
    clusters
}

/// Extract text lines from all pages of a `PagedDocument`.
///
/// For each visual line (y-group), detect whether text items form
/// multiple x-clusters (e.g., from a `#grid()` layout).  When there are
/// 2+ clusters the resulting `FrameLine` carries `x_clusters` so that
/// recovery can emit tab-separated paragraphs.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn extract_lines_from_all_pages(paged: &PagedDocument) -> Vec<FrameLine> {
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

    for page in &paged.pages {
        let mut text_items = Vec::new();
        collect_text_items_with_pos(&page.frame, Point::zero(), &mut text_items);

        let mut y_groups: BTreeMap<i64, Vec<&FrameTextItem>> = BTreeMap::new();
        for item in &text_items {
            // Use 8pt tolerance so superscript text (offset ~3-5pt up) stays grouped
            // with its base text on the same line
            let y_key = (item.y / 8.0).round() as i64;
            y_groups.entry(y_key).or_default().push(item);
        }

        for items in y_groups.values() {
            // Detect x-clusters with a 36pt (~0.5in) gap threshold.
            // Items closer than this are part of the same column.
            let page_width_pt = paged
                .pages
                .first()
                .map_or(595.0, |p| p.frame.width().to_pt());
            let max_font_size = items.iter().map(|i| i.size_pt).fold(0.0_f64, f64::max);
            // Scale gap threshold with font size: for CJK at 22pt, consecutive
            // characters are ~22pt apart, but word spacing and centering offsets
            // can create gaps up to ~5x the font size. Use max(default, font*5)
            // so large text is not falsely split into multiple "columns".
            let gap_threshold = (page_width_pt * 0.06).max(20.0).max(max_font_size * 5.0);
            let raw_clusters = cluster_by_x(items, gap_threshold);

            // Post-clustering merge: detect false splits from large-font
            // character spacing. If items have a large uniform font size,
            // the clusters are likely individual characters of a title — not
            // a real multi-column layout. Real grid columns span a significant
            // portion of the page width each.
            let clusters = if raw_clusters.len() >= 2 {
                let max_size = raw_clusters
                    .iter()
                    .flat_map(|c| c.iter().map(|i| i.size_pt))
                    .fold(0.0_f64, f64::max);
                // If the largest text on this line is significantly bigger than
                // body text, it's likely a title — don't split into tab columns.
                if max_size > body_size * 1.3 {
                    vec![raw_clusters.into_iter().flatten().collect()]
                } else {
                    raw_clusters
                }
            } else {
                raw_clusters
            };

            let mut x_clusters = Vec::new();
            let mut all_runs = Vec::new();
            let mut full_text = String::new();

            for cluster in &clusters {
                let mut cluster_runs = Vec::new();
                let cluster_x: f64 = cluster.first().map_or(0.0, |i| i.x);
                for item in cluster {
                    let is_super = item.size_pt < body_size * 0.8;
                    let mut run = Run::new(&item.text);
                    run.superscript = is_super;
                    cluster_runs.push(run.clone());
                    all_runs.push(run);
                    full_text.push_str(&item.text);
                }
                x_clusters.push(XCluster {
                    x_pt: cluster_x,
                    runs: cluster_runs,
                });
            }

            let trimmed = full_text.trim().to_string();
            if !trimmed.is_empty() {
                all_lines.push(FrameLine {
                    text: trimmed,
                    runs: all_runs,
                    x_clusters,
                });
            }
        }
    }
    all_lines
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

/// Count how many leading lines on page 1 correspond to headings in our document.
fn count_title_lines(paged_lines: &[FrameLine], doc: &Document) -> usize {
    let mut count = 0;
    for line in paged_lines {
        let is_heading = doc.body.elements.iter().any(|e| {
            if let BlockElement::Paragraph(p) = e
                && matches!(p.style, Some(ParagraphStyle::Heading(_)))
            {
                p.text_runs().any(|r| line.text.contains(&r.text))
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

/// Find which paged-line index corresponds to the start of body text.
fn find_body_start_line(paged_lines: &[FrameLine], doc: &Document) -> usize {
    let first_body_text = doc.body.elements.iter().find_map(|e| {
        if let BlockElement::Paragraph(p) = e
            && !matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            p.text_runs().next().map(|r| r.text.clone())
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

/// Extract all text from the Document model as a single normalized string.
fn extract_doc_text(doc: &Document) -> String {
    let mut text = String::new();
    for elem in &doc.body.elements {
        match elem {
            BlockElement::Paragraph(p) => {
                text.push_str(&p.text_content());
            }
            BlockElement::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for para in &cell.paragraphs {
                            text.push_str(&para.text_content());
                        }
                    }
                }
            }
        }
    }
    text
}

/// Find where the title headings end in the document (first non-heading element).
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

/// Convert a position in points to twips (1 pt = 20 twips).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pt_to_twips(pt: f64) -> u32 {
    (pt * 20.0).round().max(0.0) as u32
}

/// Insert missing lines as paragraphs at the correct position in the document.
///
/// The insertion point is after the last consecutive heading at the start of the document,
/// before the first non-heading paragraph. This matches the typical academic paper structure:
/// - Title heading
/// - Subtitle heading (optional)
/// - [INSERT HERE: author/institution info]
/// - Abstract paragraph
/// - Body content
///
/// Lines with multiple x-clusters (from `#grid()` layouts) are emitted as
/// tab-separated paragraphs with right-aligned tab stops. Lines with a single
/// cluster are emitted as centered paragraphs (the existing behaviour for
/// `#align(center)` recovery).
fn insert_missing_at_position(doc: &mut Document, missing_lines: &[FrameLine]) {
    let insert_idx = find_title_section_end(doc);

    let mut paragraphs: Vec<BlockElement> = Vec::new();
    for line in missing_lines {
        let mut para = Paragraph::new();
        para.suppress_indent = true;

        // Real grid columns each have meaningful content (>= 3 chars).
        // A title split into individual characters will have 1-2 char clusters.
        let is_real_grid = line.x_clusters.len() >= 2
            && line
                .x_clusters
                .iter()
                .all(|c| c.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() >= 3);
        if is_real_grid {
            // Multi-column line: emit runs from each cluster separated by tabs,
            // with a right-aligned tab stop for the last cluster.
            let last_cluster = &line.x_clusters[line.x_clusters.len() - 1];
            let tab_pos = pt_to_twips(last_cluster.x_pt);
            // Use the page content width as the tab stop position if we can
            // derive it; otherwise fall back to the cluster's x-position.
            // For A4 with default margins the content width is
            // (11906 - 1800 - 1800) = 8306 twips.
            let tab_stop = if tab_pos > 0 { tab_pos } else { 8306 };
            para.tab_stops.push(tab_stop);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
                    para.add_tab();
                }
                for run in &cluster.runs {
                    para.push_run(run.clone());
                }
            }
        } else {
            // Single-column line: emit as centered paragraph (existing behaviour)
            para.alignment = Some(Alignment::Center);
            for run in &line.runs {
                para.push_run(run.clone());
            }
        }
        paragraphs.push(BlockElement::Paragraph(para));
    }

    if !paragraphs.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(paragraphs);
        doc.body.elements.extend(tail);
    }
}

// ---------------------------------------------------------------------------
// Page break detection via Introspector
// ---------------------------------------------------------------------------

/// Walk the HTML tree's block-level `Tag::Start` nodes, query the
/// `PagedDocument`'s `Introspector` for each element's page number, and return
/// a `HashSet<Location>` of elements that need a page break inserted **before**
/// them (i.e. the first element on each new page after a page boundary).
///
/// Only boundaries where the previous page is *not full* (content ends before
/// 95 % of the page height) are treated as explicit page breaks.  Natural
/// page overflow (the previous page is full) is ignored because Word will
/// reflow the content naturally.
fn collect_page_break_locations(children: &[HtmlNode], paged: &PagedDocument) -> HashSet<Location> {
    // 1. Collect all block-level Tag::Start locations in document order.
    let mut locs: Vec<Location> = Vec::new();
    collect_block_tag_locations(children, &mut locs);

    if locs.is_empty() || paged.pages.len() < 2 {
        return HashSet::new();
    }

    // 2. Query the PagedDocument introspector for each location's page number.
    let page_nums: Vec<NonZeroUsize> = locs
        .iter()
        .map(|loc| paged.introspector.page(*loc))
        .collect();

    // 3. Find page boundaries: where consecutive elements jump to a higher page.
    //    For each boundary, check whether the previous page's content is short
    //    (indicating an explicit break rather than natural overflow).
    let mut result = HashSet::new();
    for i in 1..locs.len() {
        if page_nums[i] > page_nums[i - 1] {
            let prev_page_idx = page_nums[i - 1].get() - 1;
            if prev_page_idx < paged.pages.len() {
                let page = &paged.pages[prev_page_idx];
                let page_height = page.frame.height().to_pt();
                let content_y = find_max_content_y_in_frame(&page.frame, Point::zero());
                // If the previous page is nearly full (>= 95 % height used),
                // this is natural overflow — skip.
                if page_height > 0.0 && content_y >= page_height * 0.95 {
                    continue;
                }
            }
            result.insert(locs[i]);
        }
    }
    result
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

/// Find the maximum y-coordinate of any content in a frame (recursively).
fn find_max_content_y_in_frame(frame: &Frame, offset: Point) -> f64 {
    let mut max_y: f64 = 0.0;
    for (pos, item) in frame.items() {
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let y = abs_y.to_pt() + text_item.size.to_pt();
                if y > max_y {
                    max_y = y;
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(offset.x + pos.x, abs_y);
                let group_max = find_max_content_y_in_frame(&group.frame, new_offset);
                if group_max > max_y {
                    max_y = group_max;
                }
            }
            FrameItem::Image(_, size, _) => {
                let y = abs_y.to_pt() + size.y.to_pt();
                if y > max_y {
                    max_y = y;
                }
            }
            _ => {}
        }
    }
    max_y
}

// ---------------------------------------------------------------------------
// Horizontal rule detection from PagedDocument
// ---------------------------------------------------------------------------

/// Detect horizontal line shapes in the `PagedDocument` and insert horizontal
/// rule paragraphs at the corresponding positions in the document.
///
/// A horizontal rule is detected as a `FrameItem::Shape` with `Geometry::Line`
/// whose horizontal extent is at least 80% of the page content width.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn insert_horizontal_rules_from_paged(paged: &PagedDocument, doc: &mut Document) {
    let total_pages = paged.pages.len();
    if total_pages == 0 {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    // Collect all horizontal rules across all pages with their normalized position
    let mut hrules: Vec<f64> = Vec::new(); // normalized position (0.0 to 1.0 across all pages)

    for (page_idx, page) in paged.pages.iter().enumerate() {
        let page_width = page.frame.width().to_pt();
        let page_height = page.frame.height().to_pt();
        let content_width = page_width * 0.6; // Minimum 60% of page width to be a rule

        let mut lines = Vec::new();
        collect_horizontal_lines(&page.frame, Point::zero(), content_width, &mut lines);

        for line_y in lines {
            // Normalize to a position across all pages: page_idx + fraction within page
            let normalized =
                (f64::from(page_idx as u32) + line_y / page_height) / f64::from(total_pages as u32);
            hrules.push(normalized);
        }
    }

    if hrules.is_empty() {
        return;
    }

    // Insert from back to front to maintain correct indices
    hrules.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let full_doc_text = extract_doc_text(doc);

    for normalized_pos in &hrules {
        let approx_idx = (normalized_pos * total_elements as f64).round() as usize;
        let insert_idx = approx_idx.min(total_elements);

        // Don't insert if we already have a horizontal rule nearby
        // (avoid duplicates from multiple detection passes)
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

        // Skip if the document is nearly empty (no real text to separate)
        if full_doc_text.len() < 10 {
            continue;
        }

        let mut hr_para = Paragraph::new();
        hr_para.horizontal_rule = true;
        doc.body
            .elements
            .insert(insert_idx, BlockElement::Paragraph(hr_para));
    }
}

/// Recursively collect y-positions of horizontal lines from a frame.
fn collect_horizontal_lines(frame: &Frame, offset: Point, min_width: f64, lines: &mut Vec<f64>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Shape(shape, _) => {
                if let typst::visualize::Geometry::Line(end_pt) = &shape.geometry {
                    let line_width = end_pt.x.to_pt().abs();
                    let line_height = end_pt.y.to_pt().abs();
                    // Horizontal line: significant width, minimal height
                    if line_width >= min_width && line_height < 5.0 {
                        lines.push(abs_y.to_pt());
                    }
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_horizontal_lines(&group.frame, new_offset, min_width, lines);
            }
            _ => {}
        }
    }
}
