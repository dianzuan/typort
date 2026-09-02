use std::collections::{HashMap, HashSet};

use typort_ooxml::document::{Alignment, InlineElement, Paragraph, ParagraphStyle, Run};
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

use super::language::is_cjk_char;
use super::units::pt_to_half_pt;

/// Half-point fallback when no rendered body size is available.
const DEFAULT_BODY_SIZE_HALF_PT: u32 = 21;
/// OpenType weight at which rendered text is treated as bold.
const BOLD_WEIGHT_THRESHOLD: u16 = 700;
/// Binary `ital` variation-axis value at which rendered text is italic.
const ITALIC_AXIS_THRESHOLD: f32 = 0.5;
/// Fraction of page width used to recognize centered headings.
const HEADING_CENTER_TOLERANCE_RATIO: f64 = 0.05;

/// All per-run styling extracted from a single `TextItem` in the paged output.
// Each bool is an independent detected style property (weight, slant, math-face
// artifact, glyph-free), not a state machine — mirrors the same allow on `Run`.
#[allow(clippy::struct_excessive_bools)]
struct PagedRunStyle {
    text: String,
    spans: Vec<typst_syntax::Span>,
    font_family: String,
    size_pt: f64,
    color_hex: Option<String>,
    is_bold: bool,
    is_italic: bool,
    /// The face Typst shaped this run with carries an OpenType `MATH` table
    /// (`FontFlags::MATH`). This is a typographic property of the *font*, not a
    /// guess about the document: math fallback faces ("New Computer Modern
    /// Math", "Cambria Math", "STIX Two Math", …) set it; body text faces do
    /// not. When Typst's per-glyph fallback shapes an isolated glyph with such a
    /// face, copying it verbatim leaks a math font onto plain text, so we
    /// normalize it back to the baseline.
    is_math_font: bool,
    /// The run has no visible glyphs (entirely whitespace). Such a run inherits
    /// all formatting; a font/size detected on it is never observable, so it
    /// must carry no override at all.
    is_whitespace: bool,
    x: f64,
    text_width: f64,
    page_width: f64,
}

/// Collect per-run style information from all pages of the `PagedDocument`.
fn collect_paged_run_styles(paged: &PagedDocument) -> Vec<PagedRunStyle> {
    let mut items = Vec::new();
    for page in paged.pages() {
        let page_width = page.frame.width().to_pt();
        collect_styles_from_frame(&page.frame, page_width, &mut items);
    }
    items
}

fn collect_styles_from_frame(frame: &Frame, page_width: f64, items: &mut Vec<PagedRunStyle>) {
    super::frames::visit_frame_items(frame, false, &mut |position, item| {
        if let FrameItem::Text(text_item) = item {
            let text = text_item.text.to_string();
            if text.is_empty() {
                return;
            }
            let info = text_item.font.info();
            let spans: Vec<typst_syntax::Span> =
                text_item.glyphs.iter().map(|g| g.span.0).collect();
            // Compute the artifact signals before `text` is moved into the
            // struct. `text` is non-empty (early-continue above), so the
            // `all(..)` whitespace check can't vacuously succeed.
            let is_math_font = info.flags.contains(typst_library::text::FontFlags::MATH);
            let is_whitespace = text.chars().all(char::is_whitespace);
            items.push(PagedRunStyle {
                text,
                spans,
                font_family: info.family.clone(),
                size_pt: text_item.size.to_pt(),
                color_hex: extract_non_black_color(&text_item.fill),
                is_bold: effective_weight(&text_item.font) >= BOLD_WEIGHT_THRESHOLD,
                is_italic: matches!(
                    effective_style(&text_item.font),
                    typst_library::text::FontStyle::Italic
                        | typst_library::text::FontStyle::Oblique
                ),
                is_math_font,
                is_whitespace,
                x: position.x.to_pt(),
                text_width: text_item.width().to_pt(),
                page_width,
            });
        }
    });
}

/// Finds the resolved coordinate for a variation axis on a shaped
/// `FontInstance`, if the run was actually shaped with that axis set (i.e.
/// the font declares the axis at all). Shared by `effective_weight` and
/// `effective_style` below, which each read a different axis (`wght` vs.
/// `ital`/`slnt`) populated the same way at shape time —
/// `FontVariations::resolve`, typst-library `text/font/variations.rs`.
fn variation_coordinate(
    instance: &typst_library::text::FontInstance,
    tag: typst_library::text::Tag,
) -> Option<typst_library::text::AxisValue> {
    instance
        .variations()
        .0
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|&(_, value)| value)
}

/// The font weight Typst actually rendered this run with.
///
/// `TextItem::font` is a `FontInstance` (typst 0.15): a `Font` plus the
/// variation coordinates it was shaped with. For a static (non-variable)
/// font, `FontInstance::variations()` is empty, and `Font::info().variant.weight`
/// — the single weight the face declares — is authoritative.
///
/// For a variable font with a `wght` axis, though, `info()` always reports the
/// *file's default named instance* (`FontInfo::from_ttf`, computed once from the
/// raw font data, typst-library `text/font/mod.rs`), never the per-run
/// instantiation. The actual weight requested via `#text(weight: ...)` is
/// resolved into `variations()` at shape time (`FontVariations::resolve` sets the
/// `wght` axis from `variant.weight`, typst-library `text/font/variations.rs`).
/// So for a VF run we must read the resolved `wght` coordinate — falling back to
/// `info().variant.weight` when the font has no `wght` axis (variations empty).
fn effective_weight(instance: &typst_library::text::FontInstance) -> u16 {
    let wght = variation_coordinate(instance, typst_library::text::StandardAxes::WGHT)
        .map(typst_library::text::FontWeight::from_wght);
    wght.unwrap_or(instance.info().variant.weight).to_number()
}

/// The font style (italic/oblique/normal) Typst actually rendered this run
/// with.
///
/// Same class of bug as `effective_weight`, for the axis Typst resolves
/// italics onto instead of `wght`. `info().variant.style` is the file's
/// default named instance and stays `Normal` for a variable font whose
/// italics live on an `ital` or `slnt` axis rather than a separate italic
/// face — e.g. a single upright-only VF file with a `slnt` axis, which
/// typst's font selection (typst-library `text/font/book.rs`) deliberately
/// serves for an italic request when no dedicated italic face exists. The
/// actual requested style is resolved into an axis coordinate at shape time
/// (`FontVariations::resolve`, typst-library `text/font/variations.rs`):
///
/// - `ital`: set to `min(axis.max, 1.0)` for an italic request — always
///   positive when set, so any presence at `>= 0.5` signals italic (the
///   binary-axis convention; `0.5` tolerates a font whose max is below `1.0`).
/// - `slnt`: set to `axis.min` (if negative) or `axis.max` (if positive) —
///   `resolve` never assigns exactly `0` to this axis, so any nonzero
///   coordinate here means an oblique/slanted request. `resolve` only ever
///   sets one of the two axes per run (whichever its `match` picks), so the
///   two checks below are already mutually exclusive in practice; the
///   `ital`-first order matches `resolve`'s own precedence when a font
///   happens to declare both.
///
/// Falls back to `info().variant.style` when neither axis is present in
/// `variations()` (a non-VF face, or a VF with no `ital`/`slnt` axis at all).
fn effective_style(instance: &typst_library::text::FontInstance) -> typst_library::text::FontStyle {
    use typst_library::text::{FontStyle, StandardAxes};

    if variation_coordinate(instance, StandardAxes::ITAL)
        .is_some_and(|v| v.0 >= ITALIC_AXIS_THRESHOLD)
    {
        FontStyle::Italic
    } else if variation_coordinate(instance, StandardAxes::SLNT).is_some_and(|v| v.0 != 0.0) {
        FontStyle::Oblique
    } else {
        instance.info().variant.style
    }
}

struct RenderedBodyStyle {
    ascii_font: String,
    cjk_font: String,
    size_half_pt: u32,
}

/// Detect the most common font families (split by script) and size from
/// rendered text items.
///
/// Splitting by script prevents a CJK-dominated document from picking a CJK
/// fallback font as the ASCII baseline (which would cause every Latin run to
/// get a spurious font override) and vice-versa.
fn detect_rendered_body_style(styles: &[PagedRunStyle]) -> RenderedBodyStyle {
    let mut ascii_font_counts: HashMap<&str, usize> = HashMap::new();
    let mut cjk_font_counts: HashMap<&str, usize> = HashMap::new();
    let mut size_counts: HashMap<u32, usize> = HashMap::new();

    for item in styles {
        let half_pt = pt_to_half_pt(item.size_pt);
        *size_counts.entry(half_pt).or_insert(0) += item.text.len();

        let has_cjk = item.text.chars().any(is_cjk_char);
        let has_ascii = item
            .text
            .chars()
            .any(|c| c.is_ascii_alphabetic() || c.is_ascii_digit());
        if has_cjk {
            *cjk_font_counts.entry(&item.font_family).or_insert(0) += item.text.len();
        }
        if has_ascii || !has_cjk {
            *ascii_font_counts.entry(&item.font_family).or_insert(0) += item.text.len();
        }
    }

    // Tie-breaks (alphabetically-first name, smaller size) keep this detection
    // deterministic and consistent with `extract_document_style`; otherwise a
    // one-line-each document would flip body size between runs (HashMap order)
    // and shuffle which runs receive an explicit size override.
    let body_font_ascii = super::stats::dominant_key(ascii_font_counts)
        .map_or_else(|| "Times New Roman".to_string(), ToString::to_string);
    let body_font_cjk = super::stats::dominant_key(cjk_font_counts)
        .map_or_else(|| body_font_ascii.clone(), ToString::to_string);
    let body_size =
        super::stats::dominant_key(size_counts.iter().map(|(size, count)| (size, *count)))
            .map_or(DEFAULT_BODY_SIZE_HALF_PT, |size| *size);

    RenderedBodyStyle {
        ascii_font: body_font_ascii,
        cjk_font: body_font_cjk,
        size_half_pt: body_size,
    }
}

fn extract_non_black_color(paint: &typst_library::visualize::Paint) -> Option<String> {
    let typst_library::visualize::Paint::Solid(color) = paint else {
        return None;
    };
    let hex = color.to_hex();
    let hex_str = hex.as_str();
    let hex_digits = hex_str.strip_prefix('#').unwrap_or(hex_str);
    if hex_digits.starts_with("000000") {
        return None;
    }
    let rgb = &hex_digits[..6.min(hex_digits.len())];
    Some(rgb.to_uppercase())
}

/// Per-run style overrides resolved from paged output, keyed by Span or text.
#[derive(Clone)]
struct RunStyleOverride {
    color: Option<String>,
    font_ascii: Option<String>,
    font_east_asia: Option<String>,
    size_half_pt: Option<u32>,
    force_bold: Option<bool>,
    force_italic: Option<bool>,
}

struct StyleOverrideMaps {
    span_overrides: HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>>,
    text_overrides: HashMap<String, Vec<RunStyleOverride>>,
}

impl RunStyleOverride {
    fn is_empty(&self) -> bool {
        self.color.is_none()
            && self.font_ascii.is_none()
            && self.font_east_asia.is_none()
            && self.size_half_pt.is_none()
            && self.force_bold.is_none()
            && self.force_italic.is_none()
    }
}

/// Apply all per-run styles (color, font, size, bold, italic) and paragraph
/// alignment from the `PagedDocument` to the document model in a single pass.
pub fn apply_styles_from_paged(paged: &PagedDocument, doc: &mut typort_ooxml::document::Document) {
    let paged_styles = collect_paged_run_styles(paged);
    if paged_styles.is_empty() {
        return;
    }

    let rendered_body = detect_rendered_body_style(&paged_styles);
    let body_size_half_pt = rendered_body.size_half_pt;

    // When the user declares a CJK font (e.g. `font: ("Times New Roman", "SimSun")`),
    // build a set of body-level CJK fonts — any CJK font that appears at body
    // text size. These are system fallbacks for the declared font and should NOT
    // produce per-run overrides. Only activate this suppression when there IS
    // a dual-font declaration; otherwise keep all rendered font overrides.
    let has_declared_cjk_font = doc.style.body_font_ascii != doc.style.body_font_east_asia;
    let mut body_cjk_fonts: HashSet<&str> = HashSet::new();
    if has_declared_cjk_font {
        body_cjk_fonts.insert(&doc.style.body_font_east_asia);
        for item in &paged_styles {
            let size_half = pt_to_half_pt(item.size_pt);
            if size_half == body_size_half_pt && item.text.chars().any(is_cjk_char) {
                body_cjk_fonts.insert(&item.font_family);
            }
        }
    }
    let declared_ascii = &doc.style.body_font_ascii;

    let override_maps = build_style_override_maps(
        &paged_styles,
        body_size_half_pt,
        &rendered_body.ascii_font,
        &rendered_body.cjk_font,
        declared_ascii,
        &body_cjk_fonts,
    );

    // Apply run-level overrides to body elements
    apply_overrides_to_elements(&mut doc.body.elements, &override_maps);

    // Apply run-level overrides to footnotes
    for footnote in &mut doc.footnotes {
        for inline in &mut footnote.content {
            if let InlineElement::Text(run) = inline {
                apply_override_to_run(run, &override_maps, None);
            }
        }
    }

    // Apply paragraph alignment from x-positions
    apply_paragraph_alignment(&paged_styles, doc);

    // Drop per-run bold/size that merely restate the Heading style's own values:
    // the Heading{n} pStyle already supplies `<w:b/>` and the detected per-level
    // size, so repeating them on every run is noise that fights a user's Word
    // template. Only values that EQUAL the style are removed (extends commit
    // 9c458ca's "let Word own heading flow" to per-run bold/size).
    suppress_redundant_heading_run_props(doc);
}

/// Remove per-run `bold` / `size_half_pt` from runs inside heading paragraphs
/// when they exactly equal what the paragraph's `Heading{n}` style already
/// defines (`<w:b/>` plus the detected per-level size). Leaving them in place
/// fights a user's Word template and is pure noise; stripping only the values
/// that *equal* the style keeps genuinely-distinct inline styling intact (a
/// coloured or italic span, or a super/subscript whose `vertAlign` stays).
///
/// Conservative by construction: `heading_sizes[idx]` and a run's `size_half_pt`
/// are both derived from the same paged rendering, so a plain heading run's size
/// equals the style size and is dropped, while a span the author resized to
/// something else keeps its `size_half_pt`.
fn suppress_redundant_heading_run_props(doc: &mut typort_ooxml::document::Document) {
    let heading_sizes = doc.style.heading_sizes;
    for element in &mut doc.body.elements {
        let typort_ooxml::document::BlockElement::Paragraph(p) = element else {
            continue;
        };
        let Some(ParagraphStyle::Heading(level)) = p.style else {
            continue;
        };
        let idx = usize::from(level).saturating_sub(1).min(4);
        let style_size = heading_sizes[idx];
        for inline in &mut p.inlines {
            if let InlineElement::Text(run) = inline {
                strip_redundant_heading_run(run, style_size);
            }
        }
    }
}

/// Strip a single heading run's overrides that merely duplicate the style.
///
/// `bold` is cleared (the `Heading{n}` style always supplies `<w:b/>`).
/// `size_half_pt` is cleared only when it equals `style_size`. A differing size,
/// plus colour / italic / font / script overrides, are left untouched so
/// distinct inline styling inside a heading survives.
fn strip_redundant_heading_run(run: &mut Run, style_size: u32) {
    run.bold = false;
    if run.size_half_pt == Some(style_size) {
        run.size_half_pt = None;
    }
}

fn build_style_override_maps(
    paged_styles: &[PagedRunStyle],
    body_size_half_pt: u32,
    rendered_ascii: &str,
    rendered_cjk: &str,
    declared_ascii: &str,
    body_cjk_fonts: &HashSet<&str>,
) -> StyleOverrideMaps {
    let mut span_overrides: HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>> =
        HashMap::new();
    let mut text_overrides: HashMap<String, Vec<RunStyleOverride>> = HashMap::new();

    for item in paged_styles {
        // A whitespace-only run has no visible glyphs: any size/font/color
        // detected on it is a layout artifact that is never observable as
        // styling. Emit no override of any kind so it inherits cleanly (fixes
        // isolated spaces picking up a stray `sz`).
        if item.is_whitespace {
            continue;
        }

        let size_half = pt_to_half_pt(item.size_pt);

        let color = item.color_hex.clone();

        // Only set font override if different from BOTH the rendered baseline
        // AND the declared baseline. This suppresses system fallback fonts
        // (like Noto Serif SC when SimSun was declared but doesn't cover a glyph).
        let font_is_cjk = item.text.chars().any(is_cjk_char);
        let is_baseline_font = if font_is_cjk {
            if body_cjk_fonts.is_empty() {
                // No dual-font declaration — use rendered CJK baseline
                item.font_family == rendered_cjk
            } else {
                body_cjk_fonts.contains(item.font_family.as_str())
            }
        } else {
            item.font_family == rendered_ascii || item.font_family == declared_ascii
        };

        // Normalize per-glyph math-fallback artifacts back to the baseline font.
        // The signal is universal and language/genre-neutral: the shaped face
        // carries an OpenType MATH table (FontFlags::MATH). Typst's automatic
        // per-glyph fallback shapes a stray glyph (e.g. the digit `7` in "[7]")
        // with such a face; math faces lack general text coverage, so a run that
        // landed on one is a layout artifact, not authorial intent — body text
        // never falls back to a math face. We deliberately do NOT also normalize
        // "any non-letter run whose font differs from baseline": that dropped a
        // deliberate `#text(font: …)[12345]` override, and the MATH flag already
        // catches the only real artifact.
        let is_font_artifact = item.is_math_font;

        let (font_ascii, font_east_asia) = if is_baseline_font || is_font_artifact {
            (None, None)
        } else if font_is_cjk {
            (None, Some(item.font_family.clone()))
        } else {
            (Some(item.font_family.clone()), None)
        };

        let size_override = if size_half == body_size_half_pt {
            None
        } else {
            Some(size_half)
        };

        let ovr = RunStyleOverride {
            color,
            font_ascii,
            font_east_asia,
            size_half_pt: size_override,
            force_bold: if item.is_bold { Some(true) } else { None },
            force_italic: if item.is_italic { Some(true) } else { None },
        };

        if ovr.is_empty() {
            continue;
        }

        for &span in &item.spans {
            if !span.is_detached() {
                span_overrides
                    .entry(span)
                    .or_default()
                    .push((item.text.clone(), ovr.clone()));
            }
        }
        // A size override that SHRINKS below the body size is almost always a
        // super/subscript or a small positional annotation (e.g. an affiliation
        // `#super[1]` at 6.5pt). Generalizing it by TEXT pollutes same-text body
        // runs — a reference "[1]" would inherit that 6.5pt. Keep the shrink only
        // in the precise span map; the text-keyed map gets a size-stripped copy.
        let text_ovr = RunStyleOverride {
            size_half_pt: ovr.size_half_pt.filter(|&s| s >= body_size_half_pt),
            ..ovr.clone()
        };
        if !text_ovr.is_empty() {
            text_overrides
                .entry(item.text.clone())
                .or_default()
                .push(text_ovr);
        }
    }

    StyleOverrideMaps {
        span_overrides,
        text_overrides,
    }
}

fn apply_override_to_run(
    run: &mut Run,
    override_maps: &StyleOverrideMaps,
    sibling_size_hint: Option<u32>,
) {
    let ovr = run
        .span
        .and_then(|s| override_maps.span_overrides.get(&s))
        .and_then(|entries| {
            // Prefer exact text match, then substring match. Substring is only
            // meaningful for runs with visible text — a bare space is a
            // substring of nearly every paged fragment. Falling back blindly
            // to the first entry is safe only when the span is unambiguous
            // (one entry): generated content (e.g. bibliography entries) puts
            // MANY differently-styled runs on one shared span, where "first
            // entry" would smear one fragment's style across the whole block.
            entries
                .iter()
                .find(|(text, _)| text == &run.text)
                .or_else(|| {
                    if run.text.trim().is_empty() {
                        return None;
                    }
                    // Among substring matches take the LONGEST fragment — the
                    // most specific one. "First match" let a short run like
                    // "in " adopt the style of whichever unrelated fragment
                    // happened to come first in paint order.
                    entries
                        .iter()
                        .filter(|(text, _)| {
                            run.text.contains(text.as_str()) || text.contains(run.text.as_str())
                        })
                        .max_by_key(|(text, _)| text.len())
                })
                .or_else(|| (entries.len() == 1).then(|| &entries[0]))
                .map(|(_, ovr)| ovr)
        })
        .or_else(|| {
            override_maps
                .text_overrides
                .get(&run.text)
                .and_then(|entries| {
                    if entries.len() == 1 {
                        return entries.first();
                    }
                    // Multiple entries exist for the same text (e.g., "294"
                    // appears at 22pt in the title and 9pt in the abstract).
                    // Use sibling run sizes to pick the closest match.
                    if let Some(hint) = sibling_size_hint {
                        entries
                            .iter()
                            .filter(|o| o.size_half_pt.is_some())
                            .min_by_key(|o| {
                                let s = o.size_half_pt.unwrap_or(0);
                                (i64::from(s) - i64::from(hint)).unsigned_abs()
                            })
                            .or_else(|| entries.first())
                    } else {
                        // No hint available — pick the first entry
                        entries.first()
                    }
                })
        });
    let Some(ovr) = ovr else { return };

    if let Some(color) = &ovr.color {
        run.color = Some(color.clone());
    }
    if let Some(font) = &ovr.font_ascii {
        run.font_ascii = Some(font.clone());
    }
    if let Some(font) = &ovr.font_east_asia {
        run.font_east_asia = Some(font.clone());
    }
    if ovr.size_half_pt.is_some() {
        run.size_half_pt = ovr.size_half_pt;
    }
    // Bold/italic: override only when paged output disagrees with HTML semantics
    if let Some(true) = ovr.force_bold
        && !run.bold
    {
        run.bold = true;
    }
    if let Some(true) = ovr.force_italic
        && !run.italic
    {
        run.italic = true;
    }
}

fn apply_overrides_to_elements(
    elements: &mut [typort_ooxml::document::BlockElement],
    override_maps: &StyleOverrideMaps,
) {
    for element in elements.iter_mut() {
        typort_ooxml::document::for_each_paragraph_in_block_mut(element, &mut |p| {
            apply_overrides_to_paragraph(p, override_maps);
        });
    }
}

fn apply_overrides_to_paragraph(para: &mut Paragraph, override_maps: &StyleOverrideMaps) {
    // First pass: apply overrides to runs that have span-based matches or
    // unambiguous text matches.
    let mut has_ambiguous = false;
    for inline in &mut para.inlines {
        match inline {
            typort_ooxml::document::InlineElement::Text(run) => {
                let is_ambiguous = run.span.is_none()
                    && override_maps
                        .text_overrides
                        .get(&run.text)
                        .is_some_and(|e| e.len() > 1);
                if is_ambiguous {
                    has_ambiguous = true;
                } else {
                    apply_override_to_run(run, override_maps, None);
                }
            }
            typort_ooxml::document::InlineElement::Hyperlink { runs, .. } => {
                for run in runs {
                    apply_override_to_run(run, override_maps, None);
                }
            }
            _ => {}
        }
    }
    // Second pass: for runs with ambiguous text overrides, use sibling sizes
    // to pick the closest match.
    if has_ambiguous {
        // Collect the max size from sibling runs that already have overrides
        let sibling_size: Option<u32> = para
            .inlines
            .iter()
            .filter_map(|inl| {
                if let typort_ooxml::document::InlineElement::Text(r) = inl {
                    r.size_half_pt
                } else {
                    None
                }
            })
            .max();
        for inline in &mut para.inlines {
            if let typort_ooxml::document::InlineElement::Text(run) = inline {
                let is_ambiguous = run.span.is_none()
                    && override_maps
                        .text_overrides
                        .get(&run.text)
                        .is_some_and(|e| e.len() > 1);
                if is_ambiguous {
                    apply_override_to_run(run, override_maps, sibling_size);
                }
            }
        }
    }
}

/// Apply paragraph alignment by cross-referencing rendered x-positions.
fn apply_paragraph_alignment(
    paged_styles: &[PagedRunStyle],
    doc: &mut typort_ooxml::document::Document,
) {
    if paged_styles.is_empty() {
        return;
    }

    // The left text margin is the leftmost x of any rendered run: body text starts
    // at the left margin, while centered/right content starts further in. A heading
    // that starts at this margin is left-aligned even if its text happens to be
    // wide enough that its midpoint lands near the page center.
    let left_margin = paged_styles
        .iter()
        .map(|i| i.x)
        .fold(f64::INFINITY, f64::min);

    for element in &mut doc.body.elements {
        let typort_ooxml::document::BlockElement::Paragraph(p) = element else {
            continue;
        };
        if p.alignment.is_some() {
            continue;
        }
        if !matches!(p.style, Some(ParagraphStyle::Heading(_))) {
            continue;
        }

        let heading_text = p.text_content();
        if heading_text.is_empty() {
            continue;
        }

        let first_run_text = p.text_runs().next().map_or("", |r| r.text.as_str());
        if first_run_text.is_empty() {
            continue;
        }

        let heading_items: Vec<&PagedRunStyle> = paged_styles
            .iter()
            .filter(|item| {
                heading_text.contains(&item.text)
                    || item.text.contains(first_run_text)
                    || first_run_text.contains(item.text.as_str())
            })
            .collect();

        if heading_items.is_empty() {
            continue;
        }

        let min_x = heading_items
            .iter()
            .map(|i| i.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = heading_items
            .iter()
            .map(|i| i.x + i.text_width)
            .fold(f64::NEG_INFINITY, f64::max);
        let page_width = heading_items[0].page_width;

        let text_center = f64::midpoint(min_x, max_x);
        let page_center = page_width / 2.0;
        let tolerance = page_width * HEADING_CENTER_TOLERANCE_RATIO;

        // A line that begins at the left margin is left-aligned, regardless of where
        // its midpoint falls — this is what tells a wide left heading apart from a
        // genuinely centered one (which is inset from the margin on both sides).
        let starts_at_left_margin = (min_x - left_margin).abs() <= tolerance;

        if !starts_at_left_margin && (text_center - page_center).abs() < tolerance {
            p.alignment = Some(Alignment::Center);
        } else if min_x > page_center {
            p.alignment = Some(Alignment::Right);
        }
    }
}
