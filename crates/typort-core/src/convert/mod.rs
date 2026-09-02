//! Tag-walker based Typst -> OOXML conversion.
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

mod bibliography;
mod breaks;
mod coalesce;
mod fmt;
mod footnote;
mod image;
pub mod inline;
pub mod page;
mod recovery;
mod table_align;
mod table_width;

use std::collections::{HashMap, HashSet};

use std::str::FromStr;
use typort_ooxml::document::{
    Alignment, BlockElement, CellContent, Document, HangingIndent, ImageData, InlineElement,
    ListInfo, Paragraph, ParagraphStyle, Run, Table, TableCell, TableRow, VMerge,
};

use typst::comemo::Track;
use typst::foundations::{Content, NativeElement, Packed, Selector, Smart, StyleChain};
use typst::introspection::{Introspector, Location, Tag};
use typst::model::Numbering;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_layout::PagedDocument;
use typst_library::math::EquationElem;
use typst_library::model::{
    CiteElem, CiteGroup, EmphElem, HeadingElem, LinkElem, OutlineElem, RefElem, StrongElem,
    TableElem,
};
use typst_library::text::{Lang, Region, SmartQuoteElem, SmartQuoter, SmartQuotes};

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
    world: &'a TyportWorld,
    html_doc: &'a HtmlDocument,
    doc: &'a mut Document,
    eq_state: &'a mut EquationState,
    /// On-page display sizes (EMU) by image-content hash, from the paged
    /// frames. Image CONTENT comes from each `<img>`'s own src data-URL, so
    /// there is no positional queue to desync — this map only answers "how
    /// large did Typst draw these bytes".
    image_sizes: &'a HashMap<u64, (u64, u64)>,
    /// Rasterized vector-drawing canvases, keyed by their owning figure's
    /// introspection `Location` (a drawing body is dropped from the HTML
    /// export, so its pixels come from the paged frames). Keyed — not a FIFO —
    /// so a raster can only ever land on its own figure.
    figure_rasters: &'a mut HashMap<typst::introspection::Location, ImageData>,
    bookmarks: &'a mut HashSet<String>,
    /// Citation keys declared by the bibliography. A `<ref>` whose target is one
    /// of these is a citation (rendered as a marker like `[27]`), not a
    /// cross-reference to a bookmarked figure/equation/heading.
    bib_keys: &'a HashSet<String>,
}

impl WalkCtx<'_> {
    /// Add a labelled bookmark once, allocating its document-wide id here.
    fn add_bookmark(&mut self, para: &mut Paragraph, label: String) -> bool {
        if self.bookmarks.insert(label.clone()) {
            para.add_bookmark(self.doc.next_bookmark_id(), label);
            true
        } else {
            false
        }
    }
}

use crate::world::TyportWorld;

/// Rowspan metadata for a single cell: `(html_cell_index, rowspan, colspan)`.
type CellSpanInfo = (usize, u32, u32);

/// A parsed table row paired with its rowspan metadata.
type RawTableRow = (TableRow, Vec<CellSpanInfo>);

use fmt::InlineFmt;

/// Query the semantic content attached to an introspection location.
fn content_at_location(html_doc: &HtmlDocument, location: Location) -> Option<Content> {
    html_doc
        .introspector()
        .query_first(&Selector::Location(location))
}

/// Query and downcast the semantic element attached to an introspection location.
fn element_at_location<E: NativeElement>(
    html_doc: &HtmlDocument,
    location: Location,
) -> Option<Packed<E>> {
    content_at_location(html_doc, location).and_then(|content| content.into_packed::<E>().ok())
}

/// Construct a text run and retain a source span when one is attached.
fn run_with_span(text: &str, span: typst_syntax::Span) -> Run {
    let mut run = Run::new(text);
    if !span.is_detached() {
        run.span = Some(span);
    }
    run
}

/// Whether an introspected equation is block-level.
fn is_block_equation(content: &Content) -> bool {
    content
        .to_packed::<EquationElem>()
        .is_some_and(|equation| *equation.block.as_option().as_ref().unwrap_or(&false))
}

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
    let source_overrides = gather_source_overrides(world);
    apply_source_overrides(&source_overrides, &mut doc, paged_doc.is_some());

    // Note: page column count comes solely from the source AST
    // (`#set page(columns:)` / `#page(columns:)`, parsed above). There is no
    // geometric fallback — left-edge clustering cannot distinguish a real
    // multi-column page from a wide table or aligned equations, and measurement
    // showed it misread ~17 single-column fixtures as multi-column while the
    // genuine three column documents are all covered by the source parse.

    // 4. First pass: extract footnote content from <section role="doc-endnotes">,
    //    add it to the document, and size the footnote text from the Paged render.
    let body = find_body(html_doc.root()).unwrap_or_else(|| html_doc.root());
    footnote::extract_add_and_size_footnotes(&mut doc, &body.children, paged_doc.as_ref());

    // 5. From the PagedDocument: on-page image display sizes keyed by content
    //    hash (content itself comes from each <img>'s src data-URL during the
    //    walk), and drawing-canvas rasters keyed by their figure's Location.
    //    Both keyed — no positional queues to desync.
    let (image_sizes, mut figure_rasters) = if let Some(paged) = &paged_doc {
        (
            image::collect_image_sizes(paged),
            image::extract_figure_rasters(paged),
        )
    } else {
        (HashMap::new(), HashMap::new())
    };

    // 7. Walk the HTML tree's Tag sequence. Explicit `#pagebreak()` breaks are
    //    recovered from the source AST afterwards (step 12b); automatic page-flow
    //    boundaries deliberately reflow in Word rather than become hard breaks.
    let mut eq_state = EquationState::default();
    let mut bookmarks: HashSet<String> = HashSet::new();
    // Citation keys, so the <ref> handler can tell a citation from a cross-ref.
    let bib_keys: HashSet<String> = typst_library::model::BibliographyElem::keys(
        (&**html_doc.introspector() as &dyn typst_library::introspection::Introspector).track(),
    )
    .into_iter()
    .map(|(label, _)| label.resolve().to_string())
    .collect();
    {
        let mut ctx = WalkCtx {
            world,
            html_doc: &html_doc,
            doc: &mut doc,
            eq_state: &mut eq_state,
            image_sizes: &image_sizes,
            figure_rasters: &mut figure_rasters,
            bookmarks: &mut bookmarks,
            bib_keys: &bib_keys,
        };
        walk_tags(&body.children, &mut ctx);
    }

    // 8. Detect footnote format (circled numbers)
    footnote::detect_footnote_format(&body.children, &mut doc);

    // 9. Extract headers and footers from PagedDocument (before content
    //    recovery so header/footer text is not misidentified as missing body content).
    //    All margin-zone consumers share the RESOLVED margins (paged default
    //    overridden by `#set page(margin:)`, applied in step 3b above), so a
    //    small author margin never misfiles body text as header/footer.
    if let Some(paged) = &paged_doc {
        let margins = page::MarginsPt::from_settings(&doc.page_settings);
        if doc.header.is_none() {
            doc.header = page::extract_header(paged, margins);
        }
        // 9a. Detect page numbering before extracting footer.
        // If the footer is just a page number, set page_numbering instead of
        // static footer text, so the writer generates a PAGE field code.
        if let Some(fmt) = page::detect_page_numbering(paged, margins) {
            doc.page_numbering = Some(fmt);
            // Don't set doc.footer — the writer will generate a PAGE field footer
        } else if doc.footer.is_none() {
            doc.footer = page::extract_footer(paged, margins);
        }
    }

    // 10. Recover missing content from PagedDocument (e.g. #align(center) blocks)
    if let Some(paged) = &paged_doc {
        recovery::recover_missing_content(paged, &mut doc);
        // Set table borders from the rules actually drawn (three-line vs grid).
        recovery::detect_three_line_tables(paged, &mut doc);
    }

    // 11. Extract title/author from document metadata, falling back to first heading
    extract_document_metadata(&html_doc, &mut doc);

    // 11b. Extract bibliography sources for Word citation data store
    doc.citation_sources = bibliography::extract_bibliography_sources(&html_doc, world);

    // 12. Post-processing: suppress indent after headings, bibliography hanging indent
    apply_paragraph_formatting(&mut doc);

    // 12-bis. Honor `#set par(hanging-indent: …)` from the source AST. A declared
    //         hanging indent (common before a hand-written reference list) governs
    //         the paragraphs that follow it; this honors an author-stated value,
    //         not a genre heuristic.
    apply_hanging_indent_from_source(world, &mut doc);

    // 12a. Post-processing: apply per-run styles (color, font, size, bold,
    //       italic) and heading alignment from PagedDocument
    if let Some(paged) = &paged_doc {
        page::apply_styles_from_paged(paged, &mut doc);
    }

    // 12a-bis. Translate the English CJK family names Typst exposes (e.g. SimSun)
    //          into the localized name Word shows for the document's declared
    //          language (宋体). Reads each font's own name table via the font
    //          book, so it fixes the source-declared body default even when Typst
    //          substituted a different face at render time (typst#6205).
    page::localize_cjk_fonts(world, &mut doc);

    // 12c. Post-processing: detect small caps from source text
    apply_smallcaps_from_source(world, &mut doc);

    // 12c-bis. Recover explicit `#pagebreak()`/`#colbreak()` from the source
    //          AST (both are consumed during compilation, queryable in neither
    //          the HtmlDocument nor the PagedDocument), positioned by run
    //          spans and following `#include` chains. Automatic page-flow
    //          boundaries are intentionally not turned into hard breaks.
    breaks::apply_breaks_from_source(world, &mut doc);

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

    // 14. Insert horizontal rules from geometry (internally gated on a source
    //     `#line()` in ANY reachable file — main or imported/included template —
    //     so table borders / footnote separators aren't invented).
    let main_src = world.main_source().text();
    if let Some(paged) = &paged_doc {
        let main_dir = world
            .main_source()
            .id()
            .vpath()
            .realize(world.root())
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| world.root().to_path_buf());
        let sources = page::collect_reachable_source_texts(world.root(), &main_dir, main_src);
        recovery::insert_horizontal_rules_from_paged(paged, &mut doc, &element_page_map, &sources);
    }

    // 15. Merge consecutive paragraphs that belong to the same visual line
    if paged_doc.is_some() {
        recovery::merge_same_line_paragraphs(&mut doc);
    }

    // 16. Final pass: coalesce adjacent equally-formatted text runs. Runs LAST,
    //     after every per-run style patch (step 12a/12c and the recovery passes)
    //     has settled, so "equal formatting" is judged on the final styling.
    //     Covers body paragraphs, table cells, bibliography blocks, footnote
    //     bodies, and headers/footers. If any future pass re-splits runs, it must
    //     run before this one or coalescing silently undoes its work.
    coalesce::coalesce_runs(&mut doc);

    Ok(doc)
}

/// Apply authoritative values from source AST, overriding heuristic guesses.
/// Resolve the declared first-line indent (Typst default: 0pt) onto the style.
///
/// An em-based indent additionally yields a char-based `firstLineChars`
/// (`round(em × 100)`) that Word prefers, with the twips kept as a fallback.
/// Absolute (pt/cm) indents emit only twips (`first_line_indent_chars = None`).
fn apply_first_line_indent(ovr: &page::SourceStyleOverrides, doc: &mut Document, body_pt: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let indent = if let Some(em) = ovr.first_line_indent_em {
        doc.style.first_line_indent_chars = Some((em * 100.0).round() as u32);
        Some(page::pt_to_twips(em * body_pt))
    } else {
        doc.style.first_line_indent_chars = None;
        ovr.first_line_indent_twips
    };
    doc.style.first_line_indent_twips = indent.unwrap_or(0);
}

/// Split the source `#set text(font: …)` list into ASCII and East-Asian body
/// defaults.
///
/// The legacy convention `font: ("Times New Roman", "SimSun")` assumes the first
/// entry is Latin and the second CJK. That positional split breaks for a CJK-only
/// fallback list like `("NSimSun", "Noto Serif SC")`, where BOTH entries are CJK
/// and the second is just a glyph-coverage fallback that may never render. So the
/// source list is authoritative over WHICH declared name to emit, but geometry
/// (already detected into `doc.style` before this runs) decides WHICH entry
/// actually fired.
///
/// The cross-check only applies when a `PagedDocument` was compiled (`has_geometry`).
/// Without it, `doc.style` still holds the Default 宋体/Times New Roman, which
/// won't match declared names, so the HTML-only path keeps the legacy positional
/// split.
fn apply_body_font_split(fonts: &[String], doc: &mut Document, has_geometry: bool) {
    if fonts.len() < 2 {
        if let Some(f) = fonts.first() {
            doc.style.body_font_ascii.clone_from(f);
            doc.style.body_font_east_asia.clone_from(f);
        }
        return;
    }
    if !has_geometry {
        doc.style.body_font_ascii.clone_from(&fonts[0]);
        doc.style.body_font_east_asia.clone_from(&fonts[1]);
        return;
    }

    // ASCII: the declared name that matches the rendered Latin font, else fonts[0].
    let ascii = fonts
        .iter()
        .find(|f| f.eq_ignore_ascii_case(&doc.style.body_font_ascii))
        .unwrap_or(&fonts[0])
        .clone();

    // EAST-ASIA: prefer the declared name geometry says actually fired — the
    // first list entry equal to the rendered CJK font. That keeps `NSimSun` from
    // a `("NSimSun", "Noto Serif SC")` list rather than the never-rendered
    // `Noto Serif SC` fallback.
    //
    // If NO declared entry matches the rendered CJK font, Typst substituted a
    // face outside the list (the typst#6205 case, e.g. `SimSun` previews as a
    // system Mincho). The author's declared CJK name should still win over that
    // substitution, so fall back to the first declared entry that isn't the Latin
    // (ASCII) slot — the legacy positional choice — not the rendered fallback.
    let east_asia = fonts
        .iter()
        .find(|f| f.eq_ignore_ascii_case(&doc.style.body_font_east_asia))
        .or_else(|| fonts.iter().find(|f| !f.eq_ignore_ascii_case(&ascii)))
        .unwrap_or(&fonts[1])
        .clone();

    doc.style.body_font_ascii = ascii;
    doc.style.body_font_east_asia = east_asia;
}

/// Gather authoritative style overrides from the source AST: the main file plus
/// every `#import`ed file. Only document-global `set` rules count — those at a
/// file's top level or inside the closure named by the document's `#show:`
/// template (a `set text(size:)` buried in a `#block` or a non-template helper
/// closure is local and ignored). Imported files reuse the main file's template
/// names so a template library that defines the closure honors its own globals.
fn gather_source_overrides(world: &TyportWorld) -> page::SourceStyleOverrides {
    let main_text = world.main_source().text();
    let template_names = page::extract_show_template_names_from_source(main_text);
    let mut overrides = page::extract_source_style_overrides(main_text, &template_names);

    for import_path in page::extract_import_paths(main_text) {
        let abs_path = world.root().join(import_path.trim_start_matches('/'));
        if let Ok(content) = std::fs::read_to_string(&abs_path) {
            let import_overrides = page::extract_source_style_overrides(&content, &template_names);
            overrides.merge_from(&import_overrides);
        }
    }
    overrides
}

fn apply_source_overrides(
    ovr: &page::SourceStyleOverrides,
    doc: &mut Document,
    has_geometry: bool,
) {
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
    if let Some(fonts) = &ovr.text_font {
        apply_body_font_split(fonts, doc, has_geometry);
    }

    // Body text size
    if let Some(sz) = ovr.text_size_half_pt {
        doc.style.body_size_half_pt = sz;
    }

    apply_language_override(ovr, doc);

    // Resolve em-based values using actual body size
    let body_pt = f64::from(doc.style.body_size_half_pt) / 2.0;

    apply_first_line_indent(ovr, doc, body_pt);
    if let Some(all) = ovr.first_line_indent_all {
        doc.style.first_line_indent_all = all;
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
fn walk_tags(children: &[HtmlNode], ctx: &mut WalkCtx) {
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
fn handle_block_tag(
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

fn handle_table_tag(
    children: &[HtmlNode],
    i: usize,
    location: Location,
    ctx: &mut WalkCtx,
) -> usize {
    let end = find_tag_end(children, i, location);
    handle_table(&children[i..=end], Some(location), ctx);
    end
}

fn handle_list_tag(
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

fn handle_block_image(
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

fn handle_figure_or_section(
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

fn handle_outline(children: &[HtmlNode], i: usize, location: Location, ctx: &mut WalkCtx) -> usize {
    let depth = element_at_location::<OutlineElem>(ctx.html_doc, location)
        .and_then(|outline| *outline.depth.as_option())
        .flatten()
        .map_or(3, |depth| u8::try_from(depth.get()).unwrap_or(3));
    let mut para = Paragraph::new();
    para.add_toc(depth);
    ctx.doc.add_paragraph(para);
    find_tag_end(children, i, location)
}

/// Find the first nested HTML element matching `predicate`, in document order.
fn find_first_element<'a, F>(nodes: &'a [HtmlNode], predicate: &F) -> Option<&'a HtmlElement>
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
fn find_img_src(children: &[HtmlNode]) -> Option<String> {
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
fn children_are_inline(nodes: &[HtmlNode]) -> bool {
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
fn subtree_has_element(nodes: &[HtmlNode], name: &str) -> bool {
    nodes.iter().any(|node| match node {
        HtmlNode::Tag(Tag::Start(content, _)) => content.elem().name() == name,
        HtmlNode::Element(elem) => {
            tag_name(elem).as_str() == name || subtree_has_element(&elem.children, name)
        }
        _ => false,
    })
}

/// Emit only the `<figcaption>` element(s) in a figure subtree, skipping the
/// (rasterized) canvas body. Keeps the caption for vector-drawing figures while
/// dropping the canvas's leaked text labels.
fn emit_figure_caption(nodes: &[HtmlNode], ctx: &mut WalkCtx) {
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
fn handle_html_element(elem: &HtmlElement, ctx: &mut WalkCtx) {
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
fn emit_inline_equation_paragraph(
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

/// Handle a `HeadingElem` tag: query the introspector for the full Content,
/// extract level + body runs, emit a heading paragraph, and return its level.
fn handle_heading(tag: &Tag, ctx: &mut WalkCtx) -> Option<usize> {
    let content = content_at_location(ctx.html_doc, tag.location())?;
    let heading = content.to_packed::<HeadingElem>()?;

    let level = heading.resolve_level(StyleChain::default()).get();
    #[allow(clippy::cast_possible_truncation)]
    let level_u8 = level.min(255) as u8;

    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level_u8));

    // Insert bookmark if heading has a label
    if let Some(label) = content.label() {
        ctx.add_bookmark(&mut para, label.resolve().to_string());
    }

    // Prepend the synthesized heading number ("一、", "(三)", "1.1", …). Typst's
    // numbering show rule renders it but it lives in this introspector field, not
    // in `heading.body`, so the AST walk below would otherwise drop it. Language-
    // and scheme-agnostic: whatever Typst computed is emitted verbatim.
    if let Some(numbers) = heading.numbers.as_ref()
        && !numbers.is_empty()
    {
        para.push_run(Run::new(format!("{numbers} ")));
    }

    // Walk heading body — text runs, inline math, and smart quotes. Quotes live in
    // the AST as unresolved SmartQuoteElem (open/close depends on context), so we
    // resolve them with Typst's own SmartQuoter, using the document's language.
    let (lang, region) = smart_quote_lang(ctx.doc);
    let quote_dict = Smart::Auto;
    let quotes = SmartQuotes::get(&quote_dict, lang, region, false);
    let mut quote_state = SmartQuoter::new();
    // The body starts after the (optional) number prefix + space, so the first
    // quote should open: treat the preceding char as absent (the quote_state reads that
    // as a space).
    let mut prev_char: Option<char> = None;
    extract_heading_content(
        &heading.body,
        &mut para,
        &quotes,
        &mut quote_state,
        &mut prev_char,
    );
    ctx.doc.add_paragraph(para);
    Some(level)
}

/// Derive the smart-quote language/region from the document's declared language.
fn smart_quote_lang(doc: &typort_ooxml::document::Document) -> (Lang, Option<Region>) {
    let mut parts = doc.style.lang_latin.split('-');
    let lang = parts
        .next()
        .and_then(|l| Lang::from_str(l).ok())
        .unwrap_or(Lang::ENGLISH);
    let region = parts.next().and_then(|r| Region::from_str(r).ok());
    (lang, region)
}

/// Walk a heading's body content, extracting text runs, inline math, and smart
/// quotes (resolved via `quote_state`/`quotes` with `prev_char` as the preceding char).
fn extract_heading_content(
    content: &typst::foundations::Content,
    para: &mut Paragraph,
    quotes: &SmartQuotes<'_>,
    quote_state: &mut SmartQuoter,
    prev_char: &mut Option<char>,
) {
    use typst_library::foundations::SequenceElem;

    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            extract_heading_content(child, para, quotes, quote_state, prev_char);
        }
    } else if content.to_packed::<EquationElem>().is_some() {
        let omml = typort_math::equation_to_omml(content);
        para.add_math(omml);
        *prev_char = Some('\u{FFFC}'); // object replacement: an equation acts as an object
    } else if let Some(sq) = content.to_packed::<SmartQuoteElem>() {
        let double = *sq.double.as_option().as_ref().unwrap_or(&true);
        let quote = quote_state.quote(*prev_char, quotes, double);
        para.push_run(Run::new(quote));
        *prev_char = quote.chars().next_back();
    } else {
        for run in inline::extract_runs(content) {
            if let Some(c) = run.text.chars().next_back() {
                *prev_char = Some(c);
            }
            para.push_run(run);
        }
    }
}

/// Handle a `par` Tag: collect inline children (text, strong, emph, equation, footnote)
/// and emit a paragraph.
fn handle_par(slice: &[HtmlNode], ctx: &mut WalkCtx) {
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

fn strip_cjk_spaces(para: &mut Paragraph) {
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
    let Some(content) = content_at_location(html_doc, loc) else {
        return false;
    };
    !is_block_equation(&content)
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

/// The behavior needed by a caller of the shared HTML inline collector.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InlinePurpose {
    Paragraph,
    Generic,
    FormattedRun,
    Footnote,
}

/// Options for the single HTML inline collector.
#[derive(Clone, Copy)]
struct InlineOptions<'a> {
    fmt: InlineFmt,
    html_doc: Option<&'a HtmlDocument>,
    purpose: InlinePurpose,
}

impl<'a> InlineOptions<'a> {
    fn paragraph() -> Self {
        Self {
            fmt: InlineFmt::default(),
            html_doc: None,
            purpose: InlinePurpose::Paragraph,
        }
    }

    fn generic(fmt: InlineFmt, html_doc: Option<&'a HtmlDocument>) -> Self {
        Self {
            fmt,
            html_doc,
            purpose: InlinePurpose::Generic,
        }
    }

    fn formatted_run() -> Self {
        Self {
            fmt: InlineFmt::default(),
            html_doc: None,
            purpose: InlinePurpose::FormattedRun,
        }
    }

    fn footnote(fmt: InlineFmt) -> Self {
        Self {
            fmt,
            html_doc: None,
            purpose: InlinePurpose::Footnote,
        }
    }

    fn with_fmt(self, fmt: InlineFmt) -> Self {
        Self { fmt, ..self }
    }
}

/// Collect inline elements using the exact behavior selected by `options`.
fn collect_inlines(
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

fn collect_inline_element(
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

fn collect_generic_inline_tag(
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

fn collect_formatted_inline_tag(
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

fn collect_footnote_inline_tag(tag: &Tag, para: &mut Paragraph) {
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
fn content_has_equation(content: &typst::foundations::Content) -> bool {
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
fn handle_inline_tag(
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

fn handle_emphasis_tag(
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

fn handle_inline_equation_tag(
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

fn handle_inline_footnote_tag(
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

fn handle_inline_image_tag(
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

fn handle_inline_ref_tag(
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

fn handle_inline_link_tag(
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

fn handle_inline_format_tag(
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

fn handle_inline_cite_group_tag(
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

fn handle_nested_inline_tag(
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

fn handle_inline_caption_tag(
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

/// Handle a block-level equation Tag.
fn handle_equation(tag: &Tag, ctx: &mut WalkCtx) {
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
fn emit_block_equation(content: &Content, ctx: &mut WalkCtx) {
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
fn handle_block_footnote(children_from_here: &[HtmlNode], doc: &mut Document) {
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

/// Handle a `table` Tag: find the HTML `<table>` element in the inner children and parse it.
fn handle_table(slice: &[HtmlNode], table_loc: Option<Location>, ctx: &mut WalkCtx) {
    if let Some(table) = find_first_element(slice, &|element| tag_name(element) == "table") {
        convert_html_table(table, table_loc, ctx.doc, ctx.html_doc, ctx.world);
        return;
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
}

/// Handle a `list` or `enum` Tag: find the HTML `<ul>` or `<ol>` element in the inner
/// children and parse it.
fn handle_list(slice: &[HtmlNode], ordered: bool, ctx: &mut WalkCtx) {
    let list_tag = if ordered { "ol" } else { "ul" };
    if let Some(list) = find_first_element(slice, &|element| tag_name(element) == list_tag) {
        convert_html_list(list, ctx.doc, ordered, ctx.html_doc);
        return;
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
}

/// Convert an HTML `<table>` element into the document model.
fn convert_html_table(
    elem: &HtmlElement,
    table_loc: Option<Location>,
    doc: &mut Document,
    html_doc: &HtmlDocument,
    world: &TyportWorld,
) {
    let Some(mut table) = convert_html_table_to_model(elem, html_doc) else {
        return;
    };

    // Semantic column widths: read the declared track sizes off the TableElem
    // and turn them into per-cell percentages. Degrades to equal distribution
    // (cells left at width_pct = None) when the spec is all-`Auto`/`columns: N`,
    // or when the element is not queryable (e.g. nested tables with no location).
    if let Some(loc) = table_loc
        && let Some(table_elem) = element_at_location::<TableElem>(html_doc, loc)
    {
        let tracks = table_elem.columns.get_ref(StyleChain::default());
        let content_pt = f64::from(
            doc.page_settings
                .width_twips
                .saturating_sub(doc.page_settings.margin_left)
                .saturating_sub(doc.page_settings.margin_right),
        ) / 20.0;
        let wctx = table_width::TableWidthCtx {
            content_pt,
            body_font_pt: f64::from(doc.style.body_size_half_pt) / 2.0,
        };
        if let Some(col_pct) = table_width::track_widths_pct(&tracks.0, wctx) {
            table_width::assign_cell_widths(&mut table, &col_pct);
        }
        // Semantic cell alignment (the HTML `<td>`s carry none): horizontal → cell
        // paragraph `w:jc`, vertical → `w:vAlign`, read from the same TableElem.
        table_align::apply_cell_alignment(&mut table, &table_elem, world, html_doc);
    }

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
                    vertical_align: None,
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
        borders: None,
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
                    extract_cell_content_with_nested_tables(td, html_doc, paragraphs);

                cells.push(TableCell {
                    paragraphs: final_paragraphs,
                    content: cell_content,
                    colspan,
                    vmerge,
                    width_pct: None,
                    vertical_align: None,
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
    // Every paragraph collected below shares the same formatting: bold iff this
    // is a header cell, nothing else set.
    let fmt = InlineFmt {
        bold: is_header,
        ..InlineFmt::default()
    };

    // Typst's HTML export drops every equation, leaving inline math as `equation`
    // Tag siblings between the cell's <p> text fragments. The per-<p> path below
    // would consume only the <p>s — dropping those equation siblings and stacking
    // a single math-bearing line into several paragraphs. When the cell carries
    // inline math, collect the whole cell as one paragraph instead, so the
    // equations are spliced back in document order. The shared inline collector
    // already turns an `equation` Tag into OMML and recurses through <p> wrappers
    // to pick up the surrounding text.
    let has_inline_equation =
        (0..td.children.len()).any(|i| is_inline_equation_at(&td.children, i, html_doc));
    if has_inline_equation {
        let mut para = Paragraph::new();
        collect_inlines(
            &td.children,
            &mut para,
            None,
            InlineOptions::generic(fmt, Some(html_doc)),
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
                collect_inlines(
                    &el.children,
                    &mut para,
                    None,
                    InlineOptions::generic(fmt, Some(html_doc)),
                );
                if !para.inlines.is_empty() {
                    paragraphs.push(para);
                }
            }
        }
        if paragraphs.is_empty() {
            // Fallback: collect all content as one paragraph
            let mut para = Paragraph::new();
            collect_inlines(
                &td.children,
                &mut para,
                None,
                InlineOptions::generic(fmt, Some(html_doc)),
            );
            vec![para]
        } else {
            paragraphs
        }
    } else {
        let mut para = Paragraph::new();
        collect_inlines(
            &td.children,
            &mut para,
            None,
            InlineOptions::generic(fmt, Some(html_doc)),
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
    html_doc: &HtmlDocument,
    paragraphs: Vec<Paragraph>,
) -> (Vec<Paragraph>, Vec<CellContent>) {
    // Check if any child (direct or nested in a wrapper div/span) is a <table>
    // `subtree_has_element` also sees tables represented as flattened
    // `Tag::Start` markers, which an element-only walk missed.
    let has_nested_table = subtree_has_element(&td.children, "table");
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
                    collect_inlines(
                        &el.children,
                        &mut para,
                        None,
                        InlineOptions::generic(InlineFmt::default(), Some(html_doc)),
                    );
                    if !para.inlines.is_empty() {
                        content.push(CellContent::Paragraph(para));
                    }
                } else if tag != "math" {
                    // Recurse into wrapper elements (div, span, etc.). A bare
                    // `<math>` (equation outside a `<p>`) is skipped: its OMML
                    // comes from the sibling equation Tag below — recursing
                    // would leak the MathML glyphs as literal cell text.
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
            HtmlNode::Tag(tag) => {
                // A bare equation in the cell (outside any `<p>`): convert it
                // through the introspector like the inline collector does.
                if let Tag::Start(c, _) = tag
                    && c.elem().name() == "equation"
                    && let Some(eq) = content_at_location(html_doc, tag.location())
                {
                    let omml = typort_math::equation_to_omml(&eq);
                    let mut para = Paragraph::new();
                    para.add_math(omml);
                    content.push(CellContent::Paragraph(para));
                }
            }
            HtmlNode::Frame(frame) => {
                // Layouted-opaque content in a cell: rasterize in place.
                if let Some(img) = image::rasterize_html_frame(frame) {
                    let mut para = Paragraph::new();
                    para.add_image(img);
                    content.push(CellContent::Paragraph(para));
                }
            }
        }
    }
}

/// Convert an HTML `<table>` element into a `Table` model (without adding to doc).
/// Returns `None` if the table has no rows.
fn convert_html_table_to_model(elem: &HtmlElement, html_doc: &HtmlDocument) -> Option<Table> {
    let raw_rows = collect_table_rows(elem, html_doc);
    (!raw_rows.is_empty()).then(|| postprocess_rowspans(raw_rows))
}

/// Collect direct and section-wrapped HTML table rows once for every table path.
fn collect_table_rows(elem: &HtmlElement, html_doc: &HtmlDocument) -> Vec<RawTableRow> {
    let mut raw_rows = Vec::new();
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
    raw_rows
}

/// Convert an HTML `<ol>` or `<ul>` element into list paragraphs.
fn convert_html_list(
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

fn convert_html_list_at_level(
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
        // `NumberingPattern::apply` now takes a `warning_context` and returns a
        // `StrResult`; pass `None` (no engine to warn through here) and drop a
        // formatting error rather than panicking.
        pattern.apply(None, &nums).ok().map(|s| s.to_string())
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
/// (which would assume the document's language — see CLAUDE.md language-neutrality rules).
fn apply_paragraph_formatting(doc: &mut Document) {
    let mut after_heading = false;
    let mut is_first_element = true;
    // When the source declared `first-line-indent: (.., all: true)`, EVERY
    // paragraph is indented — including the first after a heading — so we must
    // not suppress it (the Typst default `all: false` does suppress it).
    let indent_all = doc.style.first_line_indent_all;

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
                    p.suppress_indent = !indent_all;
                    after_heading = false;
                }
            }
            is_first_element = false;
        } else if let BlockElement::Table(t) = element {
            // Table cells never take the body first-line indent (the cell is its
            // own context). Without this they inherit the Normal style's indent.
            suppress_table_cell_indents(t);
        }
    }
}

/// Suppress the first-line indent on every paragraph inside a table's cells,
/// recursing into nested tables.
fn suppress_table_cell_indents(table: &mut Table) {
    for row in &mut table.rows {
        for cell in &mut row.cells {
            for para in &mut cell.paragraphs {
                para.suppress_indent = true;
            }
            for content in &mut cell.content {
                match content {
                    CellContent::Paragraph(p) => p.suppress_indent = true,
                    CellContent::Table(nested) => suppress_table_cell_indents(nested),
                }
            }
        }
    }
}

/// Set the document title from the first heading's text.
/// Extract document metadata (title, author) from `#set document(...)` if present,
/// falling back to the first heading text for the title.
/// Apply `#set par(hanging-indent: …)` from the source AST to the paragraphs it
/// governs. Each rule applies from its byte offset onward; a paragraph adopts a
/// hanging indent when the last rule at or before its earliest run is non-zero.
/// Runs whose spans don't resolve into the main source (imported helper output,
/// detached content) are skipped automatically. Imported document-template set
/// rules are resolved separately and apply to the main-source body spans.
fn apply_hanging_indent_from_source(world: &TyportWorld, doc: &mut Document) {
    let source = world.main_source();
    let rules = page::collect_par_hanging_indent_rules(world);
    if rules.is_empty() {
        return;
    }
    let body_size_pt = f64::from(doc.style.body_size_half_pt) / 2.0;
    for element in &mut doc.body.elements {
        // BibliographyBlock owns its hanging indent (the doc-bibliography path);
        // only plain body paragraphs are governed here. List items, headings,
        // code blocks, and rule paragraphs carry their own indent model (a list
        // item's own list hanging indent must win, not be clobbered by this), so
        // they are skipped.
        let BlockElement::Paragraph(p) = element else {
            continue;
        };
        if p.list_info.is_some()
            || p.code_block
            || p.horizontal_rule
            || matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            continue;
        }
        // The paragraph's source position is its earliest run that resolves into
        // the main source.
        let Some(offset) = p
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                InlineElement::Text(run) => run.span,
                _ => None,
            })
            // `Source::range` now takes a decomposed `(SpanNumber, Option<SubRange>)`;
            // `WorldExt::range` does that decomposition for a `Span`. Keep the
            // main-source-only behavior by skipping spans from other files
            // (imported templates), which previously yielded `None` here.
            .filter(|span| span.id() == Some(source.id()))
            .filter_map(|span| typst_library::WorldExt::range(world, span).map(|r| r.start))
            .min()
        else {
            continue;
        };
        // The active rule is the last one at or before this paragraph. Only turn
        // the indent ON (a reset rule leaves it off); never clear one set
        // elsewhere.
        let active = rules.partition_point(|r| r.offset <= offset);
        let rule = rules[..active]
            .iter()
            .rev()
            .find(|rule| rule.scope_end.is_none_or(|end| offset < end));
        if let Some(rule) = rule.filter(|rule| rule.nonzero) {
            let relative_twips = rule.em.map_or(0, |em| page::pt_to_twips(body_size_pt * em));
            p.hanging_indent = Some(HangingIndent::Twips(
                relative_twips.saturating_add(rule.twips.unwrap_or(0)),
            ));
        }
    }
}

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
        return node.leaf_text().to_string();
    }
    let mut result = String::new();
    for child in node.children() {
        result.push_str(&collect_text_from_syntax_node(child));
    }
    result
}

fn extract_document_metadata(html_doc: &HtmlDocument, doc: &mut Document) {
    // The `info` field is now private; read it through the `Document` trait's
    // `info()` accessor. The trait is referenced fully-qualified to avoid a name
    // clash with the OOXML `Document` already in scope (the `doc` parameter).
    let info = typst_library::model::Document::info(html_doc);
    // Prefer explicit metadata from `#set document(title: ..., author: ...)`
    if let Some(title) = &info.title {
        doc.metadata.title = Some(title.to_string());
    }
    if !info.author.is_empty() {
        doc.metadata.author = Some(
            info.author
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

/// Sanitize an HTML id into a valid Word bookmark/anchor name (letters, digits and
/// underscore; not starting with a digit; <= 40 chars).
fn sanitize_anchor(id: &str) -> String {
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
fn first_biblioref_href(nodes: &[HtmlNode]) -> Option<String> {
    find_first_element(nodes, &|element| {
        tag_name(element) == "a" && has_attr_value(element, "role", "doc-biblioref")
    })
    .and_then(|element| get_attr_value(element, "href"))
}

/// Collect each `<li>`'s `id` attribute within `nodes`, in document order — the
/// bibliography entries' anchors.
fn collect_li_ids(nodes: &[HtmlNode], out: &mut Vec<Option<String>>) {
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
