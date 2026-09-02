//! Tag-walker based Typst -> OOXML conversion.
//!
//! Walks `HtmlDocument`'s `Tag` sequence. Each `Tag::Start` carries a
//! `Location` that maps via the `Introspector` to the full Content AST for
//! that element, giving us direct access to `HeadingElem`, `EquationElem`,
//! `FootnoteElem`, etc. without parsing HTML tags.

mod bibliography;
mod block;
mod breaks;
mod coalesce;
mod dom;
mod fmt;
mod footnote;
mod frames;
mod headings;
mod image;
pub mod inline;
mod inline_walk;
mod lists;
pub mod page;
mod postprocess;
mod recovery;
mod smallcaps;
mod source;
mod stats;
mod table_align;
mod table_width;
mod tables;

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

use crate::world::TyportWorld;
use fmt::InlineFmt;

use block::{
    emit_block_equation, is_inline_equation_at, strip_cjk_spaces_str, strip_visual_markers,
    walk_tags,
};
use dom::{
    children_are_inline, collect_block_tag_locations, collect_deep_text, collect_flat_text,
    collect_li_ids, content_at_location, detect_alignment, drain_text_runs, element_at_location,
    find_body, find_first_element, find_img_src, find_tag_end, first_biblioref_href,
    get_attr_value, has_attr_value, is_block_equation, is_doc_endnotes_section, run_with_span,
    sanitize_anchor, subtree_has_element, tag_name,
};
use headings::handle_heading;
use inline_walk::{InlineOptions, collect_inlines};
use lists::{convert_html_list, handle_list_tag};
use postprocess::{apply_paragraph_formatting, extract_document_metadata};
use smallcaps::apply_smallcaps_from_source;
use source::{apply_hanging_indent_from_source, apply_source_overrides, gather_source_overrides};
use tables::{convert_html_table, handle_table_tag};

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

/// Convert a Typst source file to an OOXML `Document` using the tag-walker approach.
///
/// # Errors
/// Returns compilation errors if the Typst source cannot be compiled.
pub fn convert(world: &TyportWorld) -> Result<Document, Vec<String>> {
    // 1. Compile both targets.
    let (html_doc, paged_doc) = compile_both_targets(world)?;
    let mut doc = Document::new();

    // 2. Apply page settings and document style.
    apply_page_and_style(world, paged_doc.as_ref(), &mut doc);

    // 3. Walk the HTML body.
    let body = walk_body(world, &html_doc, paged_doc.as_ref(), &mut doc);

    // 4. Apply headers and footers.
    apply_headers_and_footers(paged_doc.as_ref(), &mut doc);

    // 5. Run post-processing passes.
    run_post_passes(world, &html_doc, paged_doc.as_ref(), body, &mut doc);

    Ok(doc)
}

fn compile_both_targets(
    world: &TyportWorld,
) -> Result<(HtmlDocument, Option<PagedDocument>), Vec<String>> {
    let html_result = typst::compile::<HtmlDocument>(world);
    let html_doc = match html_result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };

    let paged_result = typst::compile::<PagedDocument>(world);
    let paged_doc = paged_result.output.ok();

    Ok((html_doc, paged_doc))
}

fn apply_page_and_style(
    world: &TyportWorld,
    paged_doc: Option<&PagedDocument>,
    doc: &mut Document,
) {
    // Extract page settings and document style from PagedDocument (heuristic).
    if let Some(paged) = paged_doc {
        doc.style = page::extract_document_style(paged);
        page::extract_page_settings(paged, &mut doc.page_settings);
    }

    // Override with authoritative values from source AST.
    let source_overrides = gather_source_overrides(world);
    apply_source_overrides(&source_overrides, doc, paged_doc.is_some());

    // Note: page column count comes solely from the source AST
    // (`#set page(columns:)` / `#page(columns:)`, parsed above). There is no
    // geometric fallback — left-edge clustering cannot distinguish a real
    // multi-column page from a wide table or aligned equations, and measurement
    // showed it misread ~17 single-column fixtures as multi-column while the
    // genuine three column documents are all covered by the source parse.
}

fn walk_body<'a>(
    world: &TyportWorld,
    html_doc: &'a HtmlDocument,
    paged_doc: Option<&PagedDocument>,
    doc: &mut Document,
) -> &'a HtmlElement {
    // First pass: extract footnote content from <section role="doc-endnotes">,
    // add it to the document, and size the footnote text from the Paged render.
    let body = find_body(html_doc.root()).unwrap_or_else(|| html_doc.root());
    footnote::extract_add_and_size_footnotes(doc, &body.children, paged_doc);

    // From the PagedDocument: on-page image display sizes keyed by content
    // hash (content itself comes from each <img>'s src data-URL during the
    // walk), and drawing-canvas rasters keyed by their figure's Location.
    // Both keyed — no positional queues to desync.
    let (image_sizes, mut figure_rasters) = if let Some(paged) = paged_doc {
        (
            image::collect_image_sizes(paged),
            image::extract_figure_rasters(paged),
        )
    } else {
        (HashMap::new(), HashMap::new())
    };

    // Walk the HTML tree's Tag sequence. Explicit `#pagebreak()` breaks are
    // recovered from the source AST during post-processing; automatic page-flow
    // boundaries deliberately reflow in Word rather than become hard breaks.
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
            html_doc,
            doc,
            eq_state: &mut eq_state,
            image_sizes: &image_sizes,
            figure_rasters: &mut figure_rasters,
            bookmarks: &mut bookmarks,
            bib_keys: &bib_keys,
        };
        walk_tags(&body.children, &mut ctx);
    }

    // Detect footnote format (circled numbers).
    footnote::detect_footnote_format(&body.children, doc);

    body
}

fn apply_headers_and_footers(paged_doc: Option<&PagedDocument>, doc: &mut Document) {
    // Extract headers and footers before content recovery so header/footer text
    // is not misidentified as missing body content. All margin-zone consumers
    // share the resolved margins (paged default overridden by the source AST),
    // so a small author margin never misfiles body text as header/footer.
    if let Some(paged) = paged_doc {
        let margins = page::MarginsPt::from_settings(&doc.page_settings);
        if doc.header.is_none() {
            doc.header = page::extract_header(paged, margins);
        }
        // Detect page numbering before extracting footer. If the footer is just
        // a page number, set page_numbering so the writer generates a PAGE field.
        if let Some(fmt) = page::detect_page_numbering(paged, margins) {
            doc.page_numbering = Some(fmt);
            // Don't set doc.footer — the writer will generate a PAGE field footer
        } else if doc.footer.is_none() {
            doc.footer = page::extract_footer(paged, margins);
        }
    }
}

fn run_post_passes(
    world: &TyportWorld,
    html_doc: &HtmlDocument,
    paged_doc: Option<&PagedDocument>,
    body: &HtmlElement,
    doc: &mut Document,
) {
    // Recover missing content from PagedDocument (e.g. #align(center) blocks).
    if let Some(paged) = paged_doc {
        recovery::recover_missing_content(paged, doc);
        // Set table borders from the rules actually drawn (three-line vs grid).
        recovery::detect_three_line_tables(paged, doc);
    }

    // Extract title/author from document metadata, falling back to first heading.
    extract_document_metadata(html_doc, doc);

    // Extract bibliography sources for Word citation data store.
    doc.citation_sources = bibliography::extract_bibliography_sources(html_doc, world);

    // Suppress indent after headings and apply bibliography hanging indent.
    apply_paragraph_formatting(doc);

    // Honor `#set par(hanging-indent: …)` from the source AST. A declared
    // hanging indent governs the paragraphs that follow it; this honors an
    // author-stated value, not a genre heuristic.
    apply_hanging_indent_from_source(world, doc);

    // Apply per-run styles and heading alignment from PagedDocument.
    if let Some(paged) = paged_doc {
        page::apply_styles_from_paged(paged, doc);
    }

    // Translate the English CJK family names Typst exposes into the localized
    // name Word shows for the document's declared language.
    page::localize_cjk_fonts(world, doc);

    // Detect small caps from source text.
    apply_smallcaps_from_source(world, doc);

    // Recover explicit `#pagebreak()`/`#colbreak()` from the source AST.
    breaks::apply_breaks_from_source(world, doc);

    // Build element→page mapping for section break and rule placement.
    let element_page_map: Vec<usize> = if let Some(paged) = paged_doc {
        recovery::build_element_page_map(doc, &body.children, paged)
    } else {
        Vec::new()
    };

    // Detect and apply section breaks from page setting changes.
    if let Some(paged) = paged_doc {
        let sections = page::detect_section_breaks(paged);
        if !sections.is_empty() {
            page::apply_section_breaks(doc, &sections, &element_page_map);
        }
    }

    // Insert horizontal rules from geometry (internally gated on a source
    // `#line()` in any reachable file so unrelated rules aren't invented).
    let main_src = world.main_source().text();
    if let Some(paged) = paged_doc {
        let main_dir = world
            .main_source()
            .id()
            .vpath()
            .realize(world.root())
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| world.root().to_path_buf());
        let sources = page::collect_reachable_source_texts(world.root(), &main_dir, main_src);
        recovery::insert_horizontal_rules_from_paged(paged, doc, &element_page_map, &sources);
    }

    // Merge consecutive paragraphs that belong to the same visual line.
    if paged_doc.is_some() {
        recovery::merge_same_line_paragraphs(doc);
    }

    // Coalesce adjacent equally-formatted text runs last, after every per-run
    // style patch and recovery pass has settled.
    coalesce::coalesce_runs(doc);
}
