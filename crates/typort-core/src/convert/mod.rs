//! Tag-walker based Typst -> OOXML conversion (v2).
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

mod bibliography;
mod footnote;
mod image;
pub mod inline;
pub mod page;
mod recovery;

use std::collections::{HashSet, VecDeque};

use typort_ooxml::document::{
    Alignment, BlockElement, CellContent, Document, ImageData, InlineElement, ListInfo, Paragraph,
    ParagraphStyle, Run, Table, TableCell, TableRow, VMerge,
};
use typst::foundations::StyleChain;
use typst::introspection::{Location, Tag};
use typst::layout::PagedDocument;
use typst::model::Numbering;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;
use typst_library::model::{CiteGroup, HeadingElem, OutlineElem, RefElem};

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

/// State threaded through the recursive tag-walker.
///
/// Replaces the `(html_doc, doc, eq_state, image_queue, bookmarks, page_breaks)`
/// parameter tuple that previously appeared on every walk/handle function. The
/// two read-only references are shared (`&`) and the four sinks are mutable
/// (`&mut`); pass the whole thing as `&mut WalkCtx`.
///
/// Borrow note: when a function both reads `ctx.html_doc.introspector` and
/// mutates a `&mut` field, lift `let html = ctx.html_doc;` first — `&HtmlDocument`
/// is `Copy`, so this severs the borrow tie at zero cost.
struct WalkCtx<'a> {
    html_doc: &'a HtmlDocument,
    page_breaks: &'a HashSet<Location>,
    doc: &'a mut Document,
    eq_state: &'a mut EquationState,
    image_queue: &'a mut VecDeque<ImageData>,
    bookmarks: &'a mut HashSet<String>,
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
    }

    // 3b. Override with authoritative values from source AST
    let main_text = world.main_source().text();
    let mut source_overrides = page::extract_source_style_overrides(main_text);

    // Also scan imported files for set rules (e.g., template libraries
    // like lib.typ that contain `set text(font: ...)` inside functions).
    for import_path in page::extract_import_paths(main_text) {
        let abs_path = world.root().join(import_path.trim_start_matches('/'));
        if let Ok(content) = std::fs::read_to_string(&abs_path) {
            let import_overrides = page::extract_source_style_overrides(&content);
            source_overrides.merge_from(&import_overrides);
        }
    }
    apply_source_overrides(&source_overrides, &mut doc);

    // Note: page column count comes solely from the source AST
    // (`#set page(columns:)` / `#page(columns:)`, parsed above). There is no
    // geometric fallback — left-edge clustering cannot distinguish a real
    // multi-column page from a wide table or aligned equations, and measurement
    // showed it misread ~17 single-column fixtures as multi-column while the
    // genuine three column documents are all covered by the source parse.

    // 4. First pass: extract footnote content from <section role="doc-endnotes">
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);
    let footnote_contents = footnote::extract_footnote_contents(&body.children);
    for content in &footnote_contents {
        doc.add_footnote(content.clone());
    }

    // 5. Extract images from PagedDocument for embedding
    let mut image_queue: VecDeque<ImageData> = if let Some(paged) = &paged_doc {
        image::extract_images_from_paged(paged).into()
    } else {
        VecDeque::new()
    };

    // 6. Pre-compute page break locations from PagedDocument introspector
    let page_breaks: HashSet<Location> = if let Some(paged) = &paged_doc {
        recovery::collect_page_break_locations(&body.children, paged)
    } else {
        HashSet::new()
    };

    // 7. Walk the HTML tree's Tag sequence (page breaks are inserted inline
    //    based on the pre-computed page_breaks set from step 6).
    let mut eq_state = EquationState::default();
    let mut bookmarks: HashSet<String> = HashSet::new();
    {
        let mut ctx = WalkCtx {
            html_doc: &html_doc,
            page_breaks: &page_breaks,
            doc: &mut doc,
            eq_state: &mut eq_state,
            image_queue: &mut image_queue,
            bookmarks: &mut bookmarks,
        };
        walk_tags(&body.children, &mut ctx);
    }

    // 8. Detect footnote format (circled numbers)
    footnote::detect_footnote_format(&body.children, &mut doc);

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
        recovery::recover_missing_content(paged, &mut doc);
    }

    // 11. Extract title/author from document metadata, falling back to first heading
    extract_document_metadata(&html_doc, &mut doc);

    // 11b. Extract bibliography sources for Word citation data store
    doc.citation_sources = bibliography::extract_bibliography_sources(&html_doc, world);

    // 12. Post-processing: suppress indent after headings, bibliography hanging indent
    apply_paragraph_formatting(&mut doc);

    // 12a. Post-processing: apply per-run styles (color, font, size, bold,
    //       italic) and heading alignment from PagedDocument
    if let Some(paged) = &paged_doc {
        page::apply_styles_from_paged(paged, &mut doc);
    }

    // 12c. Post-processing: detect small caps from source text
    apply_smallcaps_from_source(world, &mut doc);

    // 12c-bis. Post-processing: insert column breaks from source text.
    // ColbreakElem is consumed during compilation (queryable in neither the
    // HtmlDocument nor PagedDocument), so detect `#colbreak()` in the source AST
    // and re-insert it after the paragraph it followed.
    apply_column_breaks_from_source(world, &mut doc);

    // 12d. Build element→page mapping from block tag locations for precise
    //       section break and horizontal rule placement.
    let element_page_map: Vec<usize> = if let Some(paged) = &paged_doc {
        recovery::build_element_page_map(&doc, &body.children, paged)
    } else {
        Vec::new()
    };

    // 13. Detect and apply section breaks from page setting changes
    if let Some(paged) = &paged_doc {
        let sections = page::detect_section_breaks(paged);
        if !sections.is_empty() {
            page::apply_section_breaks(&mut doc, &sections, &element_page_map);
        }
    }

    // 14. Detect horizontal rules from PagedDocument and insert them
    if let Some(paged) = &paged_doc {
        recovery::insert_horizontal_rules_from_paged(paged, &mut doc, &element_page_map);
    }

    // 15. Merge consecutive paragraphs that belong to the same visual line
    if let Some(paged) = &paged_doc {
        recovery::merge_same_line_paragraphs(&mut doc, paged);
    }

    Ok(doc)
}

/// Apply authoritative values from source AST, overriding heuristic guesses.
fn apply_source_overrides(ovr: &page::SourceStyleOverrides, doc: &mut Document) {
    // Page margins
    if let Some(v) = ovr.margin_top {
        doc.page_settings.margin_top = v;
    }
    if let Some(v) = ovr.margin_bottom {
        doc.page_settings.margin_bottom = v;
    }
    if let Some(v) = ovr.margin_left {
        doc.page_settings.margin_left = v;
    }
    if let Some(v) = ovr.margin_right {
        doc.page_settings.margin_right = v;
    }

    // Columns
    if let Some(cols) = ovr.columns {
        doc.page_settings.columns = Some(cols);
    }

    // Body text font — split into ASCII and East-Asian defaults.
    // Convention: `font: ("Times New Roman", "SimSun")` means the first entry
    // is the Latin font and the second is the CJK font.
    if let Some(fonts) = &ovr.text_font {
        if fonts.len() >= 2 {
            doc.style.body_font_ascii.clone_from(&fonts[0]);
            doc.style.body_font_east_asia.clone_from(&fonts[1]);
        } else if let Some(f) = fonts.first() {
            doc.style.body_font_ascii.clone_from(f);
            doc.style.body_font_east_asia.clone_from(f);
        }
    }

    // Body text size
    if let Some(sz) = ovr.text_size_half_pt {
        doc.style.body_size_half_pt = sz;
    }

    apply_language_override(ovr, doc);

    // Resolve em-based values using actual body size
    let body_pt = f64::from(doc.style.body_size_half_pt) / 2.0;

    // First-line indent (Typst default: 0pt)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        let indent = if let Some(em) = ovr.first_line_indent_em {
            Some((em * body_pt * 20.0).round() as u32)
        } else {
            ovr.first_line_indent_twips
        };
        doc.style.first_line_indent_twips = indent.unwrap_or(0);
    }

    // Leading (in pt) — needed below for paragraph spacing calculation.
    let leading_pt = if let Some(em) = ovr.par_leading_em {
        em * body_pt
    } else if let Some(twips) = ovr.par_leading_twips {
        f64::from(twips) / 20.0
    } else {
        0.65 * body_pt
    };

    // Body paragraph spacing: Typst's par.spacing replaces leading in the gap
    // between paragraphs. Word adds w:after on top of line pitch.
    // To compensate: w:after = max(0, par_spacing - leading).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let par_spacing_pt = if let Some(em) = ovr.par_spacing_em {
        em * body_pt
    } else if let Some(twips) = ovr.par_spacing_twips {
        f64::from(twips) / 20.0
    } else {
        1.2 * body_pt
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let after_extra = if par_spacing_pt > leading_pt {
        ((par_spacing_pt - leading_pt) * 20.0).round() as u32
    } else {
        0
    };
    doc.style.body_spacing_before = 0;
    doc.style.body_spacing_after = after_extra;

    // Line spacing: cap_height (from font metrics) + leading (from source AST).
    // Typst's line pitch = cap_height × font_size + leading, where cap_height
    // is the default top-edge metric (not ascender). We emit this as
    // w:lineRule="atLeast" in twips for precise control.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        let cap_height_pt = doc.style.body_cap_height_ratio * body_pt;
        let line_pitch_pt = cap_height_pt + leading_pt;
        doc.style.line_spacing = (line_pitch_pt * 20.0).round() as u32;
    }

    // Paragraph justification
    if let Some(justify) = ovr.justify {
        doc.style.body_alignment = if justify {
            "both".to_string()
        } else {
            "left".to_string()
        };
    }

    // Heading spacing: Typst uses block-level margin collapsing.
    // In Typst, gap = descent + max(heading_above, par_spacing, leading) + ascent.
    // In Word, gap = line_pitch + body.after + heading.before.
    // Since body.after = max(0, par_spacing - leading), heading.before should
    // add just the excess of heading.above beyond (body.after + leading).
    {
        let scales = [1.4_f64, 1.2, 1.0, 1.0, 1.0];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for (level, &scale) in scales.iter().enumerate() {
            let heading_pt = f64::from(doc.style.heading_sizes[level]) / 2.0;
            let above_em = if level == 0 { 1.8 } else { 1.44 } / scale;
            let below_em = 0.75 / scale;
            let above_pt = above_em * heading_pt;
            let below_pt = below_em * heading_pt;
            let effective_after = f64::from(after_extra) / 20.0 + leading_pt;
            doc.style.heading_spacing_before[level] = if above_pt > effective_after {
                ((above_pt - effective_after) * 20.0).round() as u32
            } else {
                0
            };
            let below_effective = below_pt.max(par_spacing_pt);
            doc.style.heading_spacing_after[level] = if below_effective > leading_pt {
                ((below_effective - leading_pt) * 20.0).round() as u32
            } else {
                0
            };
        }
    }
}

/// Apply the document language from `#set text(lang:, region:)`, overriding the
/// CJK-presence heuristic. CJK languages drive Word's East-Asian tag; all others
/// drive the Latin tag (the `w:lang` w:val / w:eastAsia split). No-op when the
/// source declares no language.
fn apply_language_override(ovr: &page::SourceStyleOverrides, doc: &mut Document) {
    let Some(lang) = &ovr.text_lang else {
        return;
    };
    let tag = page::lang_region_to_bcp47(lang, ovr.text_region.as_deref());
    if matches!(lang.to_ascii_lowercase().as_str(), "zh" | "ja" | "ko") {
        doc.style.lang_east_asia = tag;
    } else {
        doc.style.lang_latin = tag;
    }
}

/// Recursively walk `HtmlNode` children, dispatching on `Tag::Start` element types.
///
/// `page_breaks` contains `Location`s of elements that should have a page
/// break inserted before them (computed by [`collect_page_break_locations`]).
#[allow(clippy::too_many_lines)]
fn walk_tags(children: &[HtmlNode], ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    // Insert a page break before this element if the
                    // introspector-based pre-pass determined there is one.
                    if ctx.page_breaks.contains(&tag.location()) {
                        let mut pb_para = Paragraph::new();
                        pb_para.add_page_break();
                        ctx.doc.add_paragraph(pb_para);
                    }
                    let elem_name = content.elem().name();
                    match elem_name {
                        "heading" => {
                            handle_heading(tag, ctx);
                            // Track chapter changes for equation numbering
                            if let Some(c) = html
                                .introspector
                                .query_first(&typst::foundations::Selector::Location(
                                    tag.location(),
                                ))
                                .and_then(|c| c.to_packed::<HeadingElem>().cloned())
                            {
                                let level = c.resolve_level(StyleChain::default()).get();
                                if level == 1 {
                                    ctx.eq_state.chapter += 1;
                                    ctx.eq_state.eq_in_chapter = 0;
                                }
                            }
                            // Skip past the heading's inner HTML elements to the matching End tag
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "par" => {
                            // Merge subsequent inline equations + par fragments
                            // into a single Word paragraph.
                            i = handle_par_with_inline_equations(children, i, ctx);
                        }
                        "equation" => {
                            handle_equation(tag, ctx);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "footnote" => {
                            handle_block_footnote(tag, &children[i..], ctx.html_doc, ctx.doc);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "table" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_table(&children[i..=end], ctx);
                            i = end;
                        }
                        "list" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(&children[i..=end], false, ctx);
                            i = end;
                        }
                        "enum" => {
                            let end = find_tag_end(children, i, tag.location());
                            handle_list(&children[i..=end], true, ctx);
                            i = end;
                        }
                        "image" => {
                            // Consume the next image from the queue extracted from PagedDocument
                            if let Some(img_data) = ctx.image_queue.pop_front() {
                                let mut para = Paragraph::new();
                                para.add_image(img_data);
                                ctx.doc.add_paragraph(para);
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
                                if !ctx.bookmarks.contains(&label_str) {
                                    ctx.bookmarks.insert(label_str.clone());
                                    let bk_id = ctx.doc.next_bookmark_id();
                                    let mut bk_para = Paragraph::new();
                                    bk_para.add_bookmark(bk_id, label_str);
                                    ctx.doc.add_paragraph(bk_para);
                                }
                            }
                            walk_tags(&children[i + 1..end], ctx);
                            i = end;
                        }
                        "outline" => {
                            let depth: u8 = html
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
                            ctx.doc.add_paragraph(para);
                            let end = find_tag_end(children, i, tag.location());
                            i = end;
                        }
                        "pagebreak" => {
                            let mut para = Paragraph::new();
                            para.add_page_break();
                            ctx.doc.add_paragraph(para);
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
                handle_html_element(elem, ctx);
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
                    ctx.doc.add_paragraph(para);
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
fn handle_html_element(elem: &HtmlElement, ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    let tag = tag_name(elem);
    match tag.as_str() {
        "pre" => convert_code_block(elem, ctx.doc),
        "blockquote" => convert_blockquote(elem, ctx),
        "dl" => convert_term_list(elem, ctx.doc),
        "ol" => convert_html_list(elem, ctx.doc, true),
        "ul" => convert_html_list(elem, ctx.doc, false),
        "table" => convert_html_table(elem, ctx.doc, html),
        "figcaption" => {
            // Collect all figcaption content into a single paragraph
            let mut para = Paragraph::new();
            para.alignment = Some(Alignment::Center);
            collect_html_inlines(&elem.children, &mut para, false, false, false);
            if !para.inlines.is_empty() {
                ctx.doc.add_paragraph(para);
            }
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
                let mut bib_paragraphs = Vec::new();
                for element in bib_elements {
                    match element {
                        BlockElement::Paragraph(p) => {
                            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                                ctx.doc.add_paragraph(p);
                            } else {
                                let mut bp = p;
                                bp.hanging_indent = true;
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
        _ => {
            // Check for alignment on this element and apply to child paragraphs
            let alignment = detect_alignment(elem);
            let start_idx = ctx.doc.body.elements.len();
            walk_tags(&elem.children, ctx);
            if let Some(align) = alignment {
                for element in &mut ctx.doc.body.elements[start_idx..] {
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
fn handle_heading(tag: &Tag, ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    let loc = tag.location();
    let Some(content) = html
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
        if !ctx.bookmarks.contains(&label_str) {
            ctx.bookmarks.insert(label_str.clone());
            let bk_id = ctx.doc.next_bookmark_id();
            para.add_bookmark(bk_id, label_str);
        }
    }

    // Walk heading body — handle both text runs and inline math
    extract_heading_content(&heading.body, &mut para);
    ctx.doc.add_paragraph(para);
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
#[allow(clippy::too_many_lines)]
fn handle_par(slice: &[HtmlNode], ctx: &mut WalkCtx) {
    let mut para = Paragraph::new();
    // Skip the first Tag::Start("par") and collect inlines from the inner nodes
    let inner = &slice[1..slice.len().saturating_sub(1)];
    collect_par_inlines(inner, ctx, &mut para);
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
fn handle_par_with_inline_equations(
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
    collect_par_inlines(inner, ctx, &mut para);

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
            if let Some(c) = html
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
            {
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
            collect_par_inlines(p_inner, ctx, &mut para);
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

fn strip_cjk_spaces(para: &mut Paragraph) {
    let mut remove_indices = Vec::new();
    for i in 1..para.inlines.len().saturating_sub(1) {
        let InlineElement::Text(run) = &para.inlines[i] else {
            continue;
        };
        if run.text.trim() != "" {
            continue;
        }
        let prev_ends_cjk = match &para.inlines[i - 1] {
            InlineElement::Text(r) => r.text.chars().last().is_some_and(page::is_cjk_char),
            _ => false,
        };
        let next_starts_cjk = match &para.inlines[i + 1] {
            InlineElement::Text(r) => r.text.chars().next().is_some_and(page::is_cjk_char),
            _ => false,
        };
        if prev_ends_cjk && next_starts_cjk {
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
fn collect_par_inlines(children: &[HtmlNode], ctx: &mut WalkCtx, para: &mut Paragraph) {
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
                    i = handle_inline_tag(tag, children, i, ctx, para);
                }
            }
            HtmlNode::Element(elem) => {
                handle_inline_html_element(elem, ctx, para);
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

/// Process a single inline `Tag::Start` within a paragraph.
/// Returns the new index (pointing at the matching End tag).
#[allow(clippy::too_many_lines)]
fn handle_inline_tag(
    tag: &Tag,
    children: &[HtmlNode],
    i: usize,
    ctx: &mut WalkCtx,
    para: &mut Paragraph,
) -> usize {
    let html = ctx.html_doc;
    let Tag::Start(content, _) = tag else {
        return i;
    };
    let elem_name = content.elem().name();
    match elem_name {
        "strong" => {
            let loc = tag.location();
            if let Some(strong) = html
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
            if let Some(emph) = html
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
            if let Some(c) = html
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
                        ctx.doc.add_paragraph(prev);
                    }
                    let eq_number = compute_equation_number(eq_packed, ctx.eq_state);
                    let mut math_para = Paragraph::new();
                    // Insert bookmark if equation has a label
                    if let Some(label) = c.label() {
                        let label_str = format!("{}", label.resolve());
                        if !ctx.bookmarks.contains(&label_str) {
                            ctx.bookmarks.insert(label_str.clone());
                            let bk_id = ctx.doc.next_bookmark_id();
                            math_para.add_bookmark(bk_id, label_str);
                        }
                    }
                    if let Some(number) = eq_number {
                        math_para.add_numbered_math(omml, number);
                    } else {
                        math_para.add_math(omml);
                    }
                    ctx.doc.add_paragraph(math_para);
                } else {
                    para.add_math(omml);
                }
            }
            find_tag_end(children, i, tag.location())
        }
        "footnote" => {
            let start_loc = tag.location();
            if let Some(id) = footnote::find_footnote_id_in_range(&children[i..]) {
                para.add_footnote_ref(id + 1);
            }
            find_tag_end(children, i, start_loc)
        }
        "image" => {
            // Inline image within a paragraph
            if let Some(img_data) = ctx.image_queue.pop_front() {
                para.add_image(img_data);
            }
            find_tag_end(children, i, tag.location())
        }
        "ref" => {
            // Cross-reference: extract target label and display text
            let end = find_tag_end(children, i, tag.location());
            let loc = tag.location();
            if let Some(c) = html
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                && let Some(ref_elem) = c.to_packed::<RefElem>()
            {
                let target_label = format!("{}", ref_elem.target.resolve());
                let display = collect_flat_text(&children[i + 1..end]);
                para.add_field_ref(target_label, display);
            }
            end
        }
        "link" => {
            // Hyperlink: extract URL and formatted display runs
            let end = find_tag_end(children, i, tag.location());
            let loc = tag.location();
            if let Some(c) = html
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
        "super" | "sub" | "raw" | "underline" | "strike" | "highlight" | "overline"
        | "smallcaps" => {
            let end = find_tag_end(children, i, tag.location());
            let text = collect_flat_text(&children[i + 1..end]);
            if !text.is_empty() {
                let mut run = Run::new(&text);
                apply_inline_format(elem_name, &mut run);
                para.push_run(run);
            }
            end
        }
        "cite-group" => {
            let end = find_tag_end(children, i, tag.location());
            let loc = tag.location();
            if let Some(c) = html
                .introspector
                .query_first(&typst::foundations::Selector::Location(loc))
                && let Some(cite_group) = c.to_packed::<CiteGroup>()
            {
                let keys: Vec<String> = cite_group
                    .children
                    .iter()
                    .map(|cite| cite.key.resolve().to_string())
                    .collect();
                let display = collect_flat_text(&children[i + 1..end]);
                if !keys.is_empty() && !display.is_empty() {
                    para.add_citation(keys, display);
                }
            }
            end
        }
        "par" => {
            // A nested `par` reached in an inline context — e.g. an author-written
            // `par()[...]` wrapping inline math, as journal templates do for the
            // abstract. Typst emits it as Start(par) … End with the prose held in an
            // `Element<p>` inside that range, interleaved with `equation` markers.
            // The default `_ =>` arm below would `find_tag_end` straight past the
            // inner nodes, dropping the prose and leaving only the equations as an
            // orphan math paragraph. Descend instead, so prose and equations are
            // collected into this same paragraph in document order.
            let end = find_tag_end(children, i, tag.location());
            collect_par_inlines(&children[i + 1..end], ctx, para);
            end
        }
        _ => {
            // Skip unknown or non-inline tags
            find_tag_end(children, i, tag.location())
        }
    }
}

/// Apply the appropriate formatting flag to a `Run` based on the inline tag name.
fn apply_inline_format(tag_name: &str, run: &mut Run) {
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
fn handle_inline_html_element(elem: &HtmlElement, ctx: &mut WalkCtx, para: &mut Paragraph) {
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
                collect_par_inlines(&elem.children, ctx, para);
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
            collect_par_inlines(&elem.children, ctx, para);
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
                        if let Some(id) = footnote::find_footnote_id_in_range(
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
fn handle_equation(tag: &Tag, ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    let loc = tag.location();
    let Some(content) = html
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
            if !ctx.bookmarks.contains(&label_str) {
                ctx.bookmarks.insert(label_str.clone());
                let bk_id = ctx.doc.next_bookmark_id();
                para.add_bookmark(bk_id, label_str);
            }
        }
        let eq_number = compute_equation_number(eq_packed, ctx.eq_state);
        if let Some(number) = eq_number {
            para.add_numbered_math(omml, number);
        } else {
            para.add_math(omml);
        }
        ctx.doc.add_paragraph(para);
    } else {
        // Inline equation at block level: wrap in a paragraph
        para.add_math(omml);
        ctx.doc.add_paragraph(para);
    }
}

/// Handle a block-level footnote Tag.
fn handle_block_footnote(
    tag: &Tag,
    children_from_here: &[HtmlNode],
    _html_doc: &HtmlDocument,
    doc: &mut Document,
) {
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
    let _ = tag;
}

/// Handle a `table` Tag: find the HTML `<table>` element in the inner children and parse it.
fn handle_table(slice: &[HtmlNode], ctx: &mut WalkCtx) {
    let html = ctx.html_doc;
    // Look for an HTML <table> element within the tag range
    for node in slice {
        if let HtmlNode::Element(elem) = node {
            let tag = tag_name(elem);
            if tag == "table" {
                convert_html_table(elem, ctx.doc, html);
                return;
            }
            // Recurse into child elements to find the table
            if find_and_convert_table_in_elem(elem, ctx.doc, html) {
                return;
            }
        }
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
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
fn handle_list(slice: &[HtmlNode], ordered: bool, ctx: &mut WalkCtx) {
    // Look for an HTML <ul> or <ol> element within the tag range
    for node in slice {
        if let HtmlNode::Element(elem) = node {
            let tag = tag_name(elem);
            if (ordered && tag == "ol") || (!ordered && tag == "ul") {
                convert_html_list(elem, ctx.doc, ordered);
                return;
            }
            // Recurse
            if find_and_convert_list_in_elem(elem, ctx.doc, ordered) {
                return;
            }
        }
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
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

    Table {
        rows: final_rows,
        width_pct: None,
        border_size: None,
    }
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
    // Typst's HTML export drops every equation, leaving inline math as `equation`
    // Tag siblings between the cell's <p> text fragments. The per-<p> path below
    // would consume only the <p>s — dropping those equation siblings and stacking
    // a single math-bearing line into several paragraphs. When the cell carries
    // inline math, collect the whole cell as one paragraph instead, so the
    // equations are spliced back in document order. collect_html_inlines_with_doc
    // already turns an `equation` Tag into OMML and recurses through <p> wrappers
    // to pick up the surrounding text.
    let has_inline_equation =
        (0..td.children.len()).any(|i| is_inline_equation_at(&td.children, i, html_doc));
    if has_inline_equation {
        let mut para = Paragraph::new();
        collect_html_inlines_with_doc(
            &td.children,
            &mut para,
            is_header,
            false,
            false,
            Some(html_doc),
        );
        return vec![para];
    }

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
    let list_id = doc.allocate_list_id(ordered);
    convert_html_list_at_level(elem, doc, 0, list_id);
}

fn convert_html_list_at_level(elem: &HtmlElement, doc: &mut Document, level: u32, list_id: u32) {
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            para.list_info = Some(ListInfo { id: list_id, level });
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
                    if sub_tag == "ul" || sub_tag == "ol" {
                        convert_html_list_at_level(sub, doc, level + 1, list_id);
                    }
                }
            }
        }
    }
}

/// Convert a `<pre>` code block into monospace paragraphs (one per line).
fn convert_code_block(elem: &HtmlElement, doc: &mut Document) {
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
fn convert_blockquote(elem: &HtmlElement, ctx: &mut WalkCtx) {
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
fn collect_deep_text(children: &[HtmlNode]) -> String {
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
fn collect_flat_text(nodes: &[HtmlNode]) -> String {
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
/// each heading.
///
/// Bibliography hanging indent is applied only to *real* bibliographies — those
/// Typst emits with the `doc-bibliography` role from `#bibliography(...)` (see
/// the `"section"` arm of `handle_html_element`). Hand-written paragraphs that
/// merely look like a reference list are, to Typst, ordinary text, so typort
/// converts them as ordinary text rather than guessing from heading keywords
/// (which would assume the document's language — see CLAUDE.md philosophy P1).
fn apply_paragraph_formatting(doc: &mut Document) {
    let mut after_heading = false;
    let mut is_first_element = true;

    for element in &mut doc.body.elements {
        if let BlockElement::Paragraph(p) = element {
            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                // Suppress above-spacing on the first heading (Typst collapses
                // block(above) with page margin at page start).
                if is_first_element {
                    p.spacing_before = Some(0);
                }
                after_heading = true;
            } else {
                // Normal paragraph
                if after_heading {
                    p.suppress_indent = true;
                    after_heading = false;
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

/// Insert a column break after the paragraph each `#colbreak()` followed.
///
/// `ColbreakElem` is consumed during compilation and is queryable in neither
/// the `HtmlDocument` nor the `PagedDocument`, so it is recovered from the
/// source AST: for each `#colbreak()` we take the text of the markup node
/// immediately before it as an anchor, then insert a column-break paragraph
/// after the matching body paragraph. Anchors are consumed in order, so
/// repeated text is handled left-to-right.
fn apply_column_breaks_from_source(world: &TyportWorld, doc: &mut Document) {
    let anchors = extract_colbreak_anchors(world.main_source().text());

    for anchor in anchors {
        // Find the first body paragraph whose text ends with the anchor, and
        // insert a column-break paragraph after it.
        let pos = doc.body.elements.iter().position(|el| {
            if let BlockElement::Paragraph(p) = el {
                let t = p.text_content();
                let t = t.trim();
                !t.is_empty() && (t == anchor || t.ends_with(anchor.as_str()))
            } else {
                false
            }
        });
        if let Some(idx) = pos {
            let mut br = Paragraph::new();
            br.add_column_break();
            doc.body
                .elements
                .insert(idx + 1, BlockElement::Paragraph(br));
        }
    }
}

/// Is this AST node a `#colbreak()` function call?
fn is_colbreak_call(node: &typst_syntax::SyntaxNode) -> bool {
    node.kind() == typst_syntax::SyntaxKind::FuncCall
        && node
            .cast::<typst_syntax::ast::FuncCall<'_>>()
            .is_some_and(|fc| {
                matches!(fc.callee(), typst_syntax::ast::Expr::Ident(i) if i.as_str() == "colbreak")
            })
}

/// Walk the AST; for each `#colbreak()`, record the trimmed text of the markup
/// node immediately before it (its anchor paragraph).
fn collect_colbreak_anchors(node: &typst_syntax::SyntaxNode, out: &mut Vec<String>) {
    // Within each node's direct children, track the most recent Text node so a
    // following colbreak call can use it as the anchor.
    let mut last_text: Option<String> = None;
    for child in node.children() {
        if child.kind() == typst_syntax::SyntaxKind::Text {
            let t = child.text().trim().to_string();
            if !t.is_empty() {
                last_text = Some(t);
            }
        } else if is_colbreak_call(child)
            && let Some(t) = last_text.take()
        {
            out.push(t);
        }
        // Recurse for nested markup (e.g. inside #page(columns: 2)[...]).
        collect_colbreak_anchors(child, out);
    }
}

/// Collect, for each `#colbreak()` call in the source, the trimmed text of the
/// markup node immediately preceding it (the anchor paragraph).
fn extract_colbreak_anchors(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);
    let mut anchors = Vec::new();
    collect_colbreak_anchors(&root, &mut anchors);
    anchors
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
pub(super) fn tag_name(elem: &HtmlElement) -> String {
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

/// Get concatenated text content from children.
pub(super) fn get_text_content(children: &[HtmlNode]) -> Option<String> {
    let mut text = String::new();
    for child in children {
        if let HtmlNode::Text(t, _) = child {
            text.push_str(t);
        }
    }
    if text.is_empty() { None } else { Some(text) }
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
