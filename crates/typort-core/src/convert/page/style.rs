//! Document-style detection from rendered `PagedDocument` frames.
//!
//! Extracts the most-common font family and size plus paragraph spacing,
//! indentation, justification, heading styles, and footnote sizing.

use std::collections::HashMap;

use typort_ooxml::document::{Document, DocumentStyle, FootnoteFormat, InlineElement};
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

use super::language::is_cjk_char;
use super::margin::{TextFragment, collect_text_fragments, find_body_zone};
use super::units::{pt_to_half_pt, pt_to_twips};

// Paged-style heuristic thresholds. Keep every tuning knob in this block.

/// Pages sampled for the glyph-weighted document-style detector.
const DOCUMENT_STYLE_SAMPLE_PAGES: usize = 3;
/// Half-point fallback when no rendered body size is available.
const DEFAULT_BODY_SIZE_HALF_PT: u32 = 21;
/// Cap-height ratio used when rendered font metrics are unavailable.
const DEFAULT_CAP_HEIGHT_RATIO: f64 = 0.66;
/// Minimum text fragments needed to infer a first-line indent.
const MIN_INDENT_FRAGMENTS: usize = 4;
/// Minimum left-edge samples needed to compare indent positions.
const MIN_INDENT_EDGES: usize = 2;
/// Minimum x clusters needed to distinguish margin and indent positions.
const MIN_INDENT_CLUSTERS: usize = 2;
/// Maximum x-distance in points for fragments to share an indent cluster.
const INDENT_CLUSTER_TOLERANCE_PT: f64 = 3.0;
/// Minimum detected first-line indent in points.
const MIN_FIRST_LINE_INDENT_PT: f64 = 1.0;
/// Maximum detected indent as a multiple of body size.
const MAX_FIRST_LINE_INDENT_BODY_MULTIPLE: f64 = 6.0;
/// Fallback first-line indent as a multiple of body size.
const DEFAULT_FIRST_LINE_INDENT_BODY_MULTIPLE: f64 = 2.0;
/// Fallback line pitch as a multiple of body size.
const DEFAULT_LINE_PITCH_BODY_MULTIPLE: f64 = 1.65;
/// Half-point tolerance for selecting body-sized lines.
const BODY_SIZE_TOLERANCE_HALF_PT: u32 = 1;
/// Minimum body baselines needed to infer line spacing.
const MIN_LINE_SPACING_BASELINES: usize = 2;
/// Y-distance in points below which repeated baselines are deduplicated.
const LINE_Y_DEDUP_TOLERANCE_PT: f64 = 0.5;
/// Smallest plausible line pitch as a multiple of body size.
const MIN_LINE_PITCH_BODY_MULTIPLE: f64 = 0.8;
/// Largest plausible line pitch as a multiple of body size.
const MAX_LINE_PITCH_BODY_MULTIPLE: f64 = 3.0;
/// Minimum emitted line spacing in twips.
const MIN_LINE_SPACING_TWIPS: u32 = 160;
/// Maximum emitted line spacing in twips.
const MAX_LINE_SPACING_TWIPS: u32 = 960;
/// Pages sampled by the independently line-weighted justification detector.
const JUSTIFICATION_SAMPLE_PAGES: usize = 3;
/// Minimum rendered items needed to attempt justification detection.
const MIN_JUSTIFICATION_ITEMS: usize = 4;
/// Maximum baseline difference in points for items on the same line.
const JUSTIFICATION_LINE_Y_TOLERANCE_PT: f64 = 2.0;
/// Minimum line count needed after grouping or full-line filtering.
const MIN_JUSTIFICATION_LINES: usize = 3;
/// Fraction of the median right edge a line must reach to count as full.
const FULL_LINE_RIGHT_EDGE_RATIO: f64 = 0.85;
/// Maximum right-edge standard deviation in points for justified text.
const JUSTIFIED_RIGHT_EDGE_STD_DEV_PT: f64 = 3.0;
/// Smallest plausible code or footnote size in half-points.
const MIN_AUXILIARY_TEXT_SIZE_HALF_PT: u32 = 12;
/// Body-size decrement used by code and footnote fallbacks.
const AUXILIARY_TEXT_SIZE_DECREMENT_HALF_PT: u32 = 3;
/// Minimum fallback code or footnote size in half-points.
const MIN_AUXILIARY_FALLBACK_SIZE_HALF_PT: u32 = 14;
/// Minimum global count for a footnote-size fallback candidate.
const MIN_FOOTNOTE_SIZE_COUNT: usize = 3;
/// Minimum semantic footnote text length used for paged matching.
const MIN_FOOTNOTE_TEXT_CHARS: usize = 4;
/// Minimum rendered footnote fragment length used for paged matching.
const MIN_FOOTNOTE_FRAGMENT_CHARS: usize = 3;
/// Fallback heading-size offsets from the body size, in half-points.
const DEFAULT_HEADING_SIZE_OFFSETS: [u32; 5] = [9, 7, 5, 3, 1];
/// Pages sampled for heading-gap detection.
const HEADING_SPACING_SAMPLE_PAGES: usize = 5;
/// Minimum items needed to infer heading spacing.
const MIN_HEADING_SPACING_ITEMS: usize = 3;
/// Y-distance in points below which spacing samples are deduplicated.
const SPACING_Y_DEDUP_TOLERANCE_PT: f64 = 1.0;
/// Largest heading-neighbor gap as a multiple of body size.
const MAX_HEADING_GAP_BODY_MULTIPLE: f64 = 15.0;
/// Maximum inferred heading-before spacing in twips.
const MAX_HEADING_BEFORE_TWIPS: u32 = 1500;
/// Maximum inferred heading-after spacing in twips.
const MAX_HEADING_AFTER_TWIPS: u32 = 800;
/// Pages sampled by the independently gap-weighted body-spacing detector.
const BODY_SPACING_SAMPLE_PAGES: usize = 3;
/// Minimum items needed to infer body paragraph spacing.
const MIN_BODY_SPACING_ITEMS: usize = 4;
/// Smallest paragraph gap as a multiple of body size.
const MIN_PARAGRAPH_GAP_BODY_MULTIPLE: f64 = 1.8;
/// Largest paragraph gap as a multiple of body size.
const MAX_PARAGRAPH_GAP_BODY_MULTIPLE: f64 = 8.0;
/// Maximum inferred body paragraph spacing in twips.
const MAX_BODY_SPACING_TWIPS: u32 = 1000;

#[derive(Default)]
struct FontInfoHistograms {
    ascii_fonts: HashMap<String, usize>,
    cjk_fonts: HashMap<String, usize>,
    size_counts: HashMap<u32, usize>,
    y_positions: Vec<(f64, u32)>,
    body_cap_heights: Vec<f64>,
}

/// Extract document style (fonts, sizes, spacing) from the rendered `PagedDocument`.
///
/// Walks the first few pages' frames to find the most common font family and size,
/// which represent the body text styling.
#[must_use]
pub fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
    let mut histograms = FontInfoHistograms::default();

    for page in paged.pages().iter().take(DOCUMENT_STYLE_SAMPLE_PAGES) {
        collect_font_info_split(&page.frame, &mut histograms);
    }

    let FontInfoHistograms {
        ascii_fonts: ascii_font_counts,
        cjk_fonts: cjk_font_counts,
        size_counts,
        y_positions,
        body_cap_heights,
    } = histograms;

    // Detect body font (most common per script). On a count tie, fall back to
    // the alphabetically-first name so the choice is deterministic across runs
    // (a bare `max_by_key` on count would pick whichever the HashMap happened to
    // iterate first).
    let body_font_ascii = super::stats::dominant_key(
        ascii_font_counts
            .iter()
            .map(|(font, count)| (font.as_str(), *count)),
    )
    .map_or_else(|| "Times New Roman".to_string(), ToString::to_string);

    let body_font_east_asia = super::stats::dominant_key(
        cjk_font_counts
            .iter()
            .map(|(font, count)| (font.as_str(), *count)),
    )
    .map_or_else(|| body_font_ascii.clone(), ToString::to_string);

    // Detect body size (most common). On a count tie, prefer the smaller size:
    // body text is the baseline, and emphasis/headings are larger. The
    // `Reverse(size)` tie-break also makes the result deterministic — without it
    // a single heading line tying the body line (e.g. a one-line document) would
    // flip the detected body size between runs (HashMap iteration order).
    let body_size_half_pt =
        super::stats::dominant_key(size_counts.iter().map(|(size, count)| (size, *count)))
            .map_or(DEFAULT_BODY_SIZE_HALF_PT, |size| *size);

    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = detect_first_line_indent(paged, body_pt);
    let line_spacing = detect_line_spacing(&y_positions, body_size_half_pt);

    // Determine body font's cap-height ratio from collected metrics.
    // Cap-height is the text box height Typst uses for line layout.
    let body_cap_height_ratio = if body_cap_heights.is_empty() {
        DEFAULT_CAP_HEIGHT_RATIO
    } else {
        let mut sorted = body_cap_heights.clone();
        super::stats::median(&mut sorted).unwrap_or(DEFAULT_CAP_HEIGHT_RATIO)
    };

    // Detect code font (monospace font that isn't the body font)
    let code_font = super::stats::dominant_key(
        ascii_font_counts
            .iter()
            .filter(|(f, _)| {
                let fl = f.to_lowercase();
                (fl.contains("mono")
                    || fl.contains("courier")
                    || fl.contains("consol")
                    || fl.contains("fira code")
                    || fl.contains("source code"))
                    && f.as_str() != body_font_ascii
            })
            .map(|(font, count)| (font.as_str(), *count)),
    )
    .map_or_else(|| "Courier New".to_string(), ToString::to_string);

    // Detect sizes for code, footnotes, headings from actual rendered data
    let code_size_half_pt = detect_code_size(&size_counts, body_size_half_pt);
    let footnote_size_half_pt = detect_footnote_size(&size_counts, body_size_half_pt);
    let heading_sizes = detect_heading_sizes(&size_counts, body_size_half_pt);

    // Detect heading before/after spacing from y-position gaps around large text
    let (heading_spacing_before, heading_spacing_after) =
        detect_heading_spacing_per_level(body_size_half_pt, &heading_sizes, paged);

    // Detect body paragraph spacing from y-position gaps between normal text
    let (body_spacing_before, body_spacing_after) =
        detect_body_paragraph_spacing(body_size_half_pt, &heading_sizes, paged);

    // Detect CJK content presence from rendered text
    let has_cjk = !ascii_font_counts.is_empty() && cjk_font_counts.values().sum::<usize>() > 0;

    DocumentStyle {
        body_font_ascii,
        body_font_east_asia,
        body_size_half_pt,
        line_spacing,
        first_line_indent_twips,
        // Geometry detection cannot recover em units; the source-AST override
        // (apply_source_overrides) sets this when an em indent was declared.
        first_line_indent_chars: None,
        first_line_indent_all: false,
        footnote_format: FootnoteFormat::default(),
        code_font,
        body_spacing_before,
        body_spacing_after,
        heading_spacing_before,
        heading_spacing_after,
        code_size_half_pt,
        footnote_size_half_pt,
        heading_sizes,
        body_alignment: detect_justification(paged),
        lang_latin: "en-US".to_string(),
        lang_east_asia: if has_cjk {
            "zh-CN".to_string()
        } else {
            "en-US".to_string()
        },
        has_cjk_content: has_cjk,
        hyperlink_color: "0563C1".to_string(),
        body_cap_height_ratio,
    }
}

fn detect_first_line_indent(paged: &PagedDocument, body_pt: f64) -> u32 {
    let Some(page) = paged.pages().first() else {
        return pt_to_twips(body_pt * DEFAULT_FIRST_LINE_INDENT_BODY_MULTIPLE);
    };

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, &mut fragments);

    if fragments.len() < MIN_INDENT_FRAGMENTS {
        return pt_to_twips(body_pt * DEFAULT_FIRST_LINE_INDENT_BODY_MULTIPLE);
    }

    // Group by y (lines), find the left-most x per line.
    // Default-margin body zone: this runs during style extraction, before the
    // source AST margins are resolved. It only shapes an indent heuristic that
    // `#set par(first-line-indent:)` overrides anyway.
    let page_width = page.frame.width().to_pt();
    let (body_top, body_bottom) =
        find_body_zone(page_width, page.frame.height().to_pt(), None, None);

    let body_frags: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| f.y >= body_top && f.y <= body_bottom)
        .collect();

    let mut left_edges: Vec<f64> = body_frags.iter().map(|f| f.x).collect();
    left_edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if left_edges.len() < MIN_INDENT_EDGES {
        return pt_to_twips(body_pt * DEFAULT_FIRST_LINE_INDENT_BODY_MULTIPLE);
    }

    // Find the two most common left-edge positions (body margin + indented margin)
    let mut x_clusters: Vec<(f64, usize)> = Vec::new();
    for &x in &left_edges {
        if let Some(c) = x_clusters
            .iter_mut()
            .find(|(cx, _)| (x - *cx).abs() < INDENT_CLUSTER_TOLERANCE_PT)
        {
            c.1 += 1;
        } else {
            x_clusters.push((x, 1));
        }
    }
    x_clusters.sort_by_key(|b| std::cmp::Reverse(b.1));

    if x_clusters.len() >= MIN_INDENT_CLUSTERS {
        let margin_x = x_clusters[0].0.min(x_clusters[1].0);
        let indent_x = x_clusters[0].0.max(x_clusters[1].0);
        let indent_pt = indent_x - margin_x;
        if indent_pt > MIN_FIRST_LINE_INDENT_PT
            && indent_pt < body_pt * MAX_FIRST_LINE_INDENT_BODY_MULTIPLE
        {
            return pt_to_twips(indent_pt);
        }
    }

    // Fallback: 2 chars wide
    pt_to_twips(body_pt * DEFAULT_FIRST_LINE_INDENT_BODY_MULTIPLE)
}

fn detect_line_spacing(y_positions: &[(f64, u32)], body_size_half_pt: u32) -> u32 {
    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let default_twips = pt_to_twips(body_pt * DEFAULT_LINE_PITCH_BODY_MULTIPLE);

    // Filter to only body-sized text items (within ±1 half-point of detected body size)
    let mut body_ys: Vec<f64> = y_positions
        .iter()
        .filter(|(_, sz)| sz.abs_diff(body_size_half_pt) <= BODY_SIZE_TOLERANCE_HALF_PT)
        .map(|(y, _)| *y)
        .collect();

    if body_ys.len() < MIN_LINE_SPACING_BASELINES {
        return default_twips;
    }
    body_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    body_ys.dedup_by(|a, b| (*a - *b).abs() < LINE_Y_DEDUP_TOLERANCE_PT);

    let mut gaps: Vec<f64> = Vec::new();
    for pair in body_ys.windows(2) {
        let gap = pair[1] - pair[0];
        if gap > body_pt * MIN_LINE_PITCH_BODY_MULTIPLE
            && gap < body_pt * MAX_LINE_PITCH_BODY_MULTIPLE
        {
            gaps.push(gap);
        }
    }
    if gaps.is_empty() {
        return default_twips;
    }
    // Use mode (most common gap, rounded to 0.5pt) for robustness against
    // mixed within-paragraph and between-paragraph gaps.
    let mut gap_counts: HashMap<u32, usize> = HashMap::new();
    for &g in &gaps {
        let key = pt_to_half_pt(g);
        *gap_counts.entry(key).or_insert(0) += 1;
    }
    let mode_key = super::stats::dominant_key(gap_counts.iter().map(|(key, count)| (key, *count)))
        .map_or(0, |key| *key);
    let mode_pitch = f64::from(mode_key) / 2.0;
    let spacing = pt_to_twips(mode_pitch);
    spacing.clamp(MIN_LINE_SPACING_TWIPS, MAX_LINE_SPACING_TWIPS)
}

/// Detect whether the document body text is justified or left-aligned.
///
/// Justified text has uniform right edges (the right margin of each line is
/// approximately the same). Left-aligned (ragged) text has varying right edges.
/// We measure the standard deviation of right-edge x-positions of body-sized
/// text lines across the first few pages. A small standard deviation indicates
/// justified text; a larger one indicates ragged (left-aligned) text.
///
/// Returns `"both"` for justified, `"left"` for left-aligned.
fn detect_justification(paged: &PagedDocument) -> String {
    // Collect right-edge x-positions of body text lines from the first few pages.
    // We group text items by y-position (line), then compute the right edge per line.
    let mut line_items: Vec<(f64, f64)> = Vec::new(); // (y, right_edge_x)

    for page in paged.pages().iter().take(JUSTIFICATION_SAMPLE_PAGES) {
        let page_width = page.frame.width().to_pt();
        let page_height = page.frame.height().to_pt();
        // Default-margin body zone (see detect_first_line_indent): style-time
        // heuristic only, overridden by `#set par(justify:)` when declared.
        let (body_top, body_bottom) = find_body_zone(page_width, page_height, None, None);
        collect_right_edges(&page.frame, body_top, body_bottom, &mut line_items);
    }

    if line_items.len() < MIN_JUSTIFICATION_ITEMS {
        // Not enough data to decide; Typst default is left-aligned
        return "left".to_string();
    }

    // Group by y-position (same line) and compute the max right edge per line.
    line_items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut line_right_edges: Vec<f64> = Vec::new();
    let mut current_y = line_items[0].0;
    let mut current_max_x = line_items[0].1;
    for &(y, right_x) in &line_items[1..] {
        if (y - current_y).abs() <= JUSTIFICATION_LINE_Y_TOLERANCE_PT {
            // Same line: update max right edge
            if right_x > current_max_x {
                current_max_x = right_x;
            }
        } else {
            // New line: save previous line's right edge
            line_right_edges.push(current_max_x);
            current_y = y;
            current_max_x = right_x;
        }
    }
    line_right_edges.push(current_max_x); // last line

    if line_right_edges.len() < MIN_JUSTIFICATION_LINES {
        return "left".to_string();
    }

    // Exclude the last line of each paragraph — it's typically shorter.
    // We detect paragraph-final lines as lines whose right edge is significantly
    // shorter than the median right edge.
    let median_right = super::stats::median(&mut line_right_edges).unwrap_or(0.0);

    // Keep only lines whose right edge is within 85% of the median (full lines).
    let full_lines: Vec<f64> = line_right_edges
        .iter()
        .copied()
        .filter(|&x| x >= median_right * FULL_LINE_RIGHT_EDGE_RATIO)
        .collect();

    if full_lines.len() < MIN_JUSTIFICATION_LINES {
        return "left".to_string();
    }

    let std_dev = super::stats::standard_deviation(&full_lines).unwrap_or(f64::INFINITY);

    // Justified text: right edges are very uniform (std_dev < 3pt).
    // Ragged text: right edges vary by many points (std_dev > 5pt typically).
    if std_dev < JUSTIFIED_RIGHT_EDGE_STD_DEV_PT {
        "both".to_string()
    } else {
        "left".to_string()
    }
}

/// Collect (y, `right_edge_x`) pairs for text items in the body zone.
fn collect_right_edges(
    frame: &Frame,
    body_top: f64,
    body_bottom: f64,
    items: &mut Vec<(f64, f64)>,
) {
    super::frames::visit_frame_items(frame, false, &mut |position, item| {
        if let FrameItem::Text(text_item) = item {
            let y = position.y.to_pt();
            if y >= body_top && y <= body_bottom {
                let right_edge = position.x.to_pt() + text_item.width().to_pt();
                items.push((y, right_edge));
            }
        }
    });
}

/// Detect code block font size: the most common size smaller than body that's used with mono fonts.
/// Falls back to `body_size - 3` half-points.
fn detect_code_size(size_counts: &HashMap<u32, usize>, body_size: u32) -> u32 {
    super::stats::dominant_key(
        size_counts
            .iter()
            .filter(|(size, _)| **size < body_size && **size >= MIN_AUXILIARY_TEXT_SIZE_HALF_PT)
            .map(|(size, count)| (size, *count)),
    )
    .map_or(
        body_size
            .saturating_sub(AUXILIARY_TEXT_SIZE_DECREMENT_HALF_PT)
            .max(MIN_AUXILIARY_FALLBACK_SIZE_HALF_PT),
        |size| *size,
    )
}

/// Fallback footnote text size from the global histogram: the smallest size with
/// significant usage. This is imprecise — it cannot tell the footnote body text
/// from the (smaller) superscript reference marker — so it is only used when the
/// semantic [`detect_footnote_text_size`] finds no footnote text to measure.
fn detect_footnote_size(size_counts: &HashMap<u32, usize>, body_size: u32) -> u32 {
    size_counts
        .iter()
        .filter(|(sz, count)| {
            **sz < body_size
                && **count >= MIN_FOOTNOTE_SIZE_COUNT
                && **sz >= MIN_AUXILIARY_TEXT_SIZE_HALF_PT
        })
        .min_by_key(|(sz, _)| *sz)
        .map_or(
            body_size
                .saturating_sub(AUXILIARY_TEXT_SIZE_DECREMENT_HALF_PT)
                .max(MIN_AUXILIARY_FALLBACK_SIZE_HALF_PT),
            |(sz, _)| *sz,
        )
}

/// Refine `doc.style.footnote_size_half_pt` from the actual footnote entries when a
/// Paged render is available — overriding the imprecise global-histogram fallback
/// with the footnote body size measured by [`detect_footnote_text_size`].
pub(in crate::convert) fn apply_footnote_text_size(
    doc: &mut Document,
    paged: Option<&PagedDocument>,
    footnote_contents: &[Vec<InlineElement>],
) {
    if let Some(paged) = paged
        && let Some(sz) = detect_footnote_text_size(paged, footnote_contents)
    {
        doc.style.footnote_size_half_pt = sz;
    }
}

/// Measure the footnote **body** text size from the Paged render, located by the
/// already-extracted footnote content. The global histogram ([`detect_footnote_size`])
/// takes the smallest size in the document, which is the superscript reference
/// marker — not the footnote text. Matching the rendered fragments against the
/// semantic footnote content reads the size of the actual footnote runs instead.
/// Returns the dominant matched size, or `None` when nothing matches (caller keeps
/// the histogram fallback).
fn detect_footnote_text_size(
    paged: &PagedDocument,
    footnote_contents: &[Vec<InlineElement>],
) -> Option<u32> {
    let mut haystack = String::new();
    for content in footnote_contents {
        for inline in content {
            if let InlineElement::Text(run) = inline {
                haystack.push_str(&run.text);
            }
        }
    }
    let haystack: String = haystack.chars().filter(|c| !c.is_whitespace()).collect();
    if haystack.chars().count() < MIN_FOOTNOTE_TEXT_CHARS {
        return None;
    }
    let mut size_counts: HashMap<u32, usize> = HashMap::new();
    for page in paged.pages() {
        collect_footnote_text_sizes(&page.frame, &haystack, &mut size_counts);
    }
    // The footnote body is the dominant matched size; a tie prefers the smaller
    // size (footnotes are smaller than any body text that coincidentally matches).
    super::stats::dominant_key(size_counts.iter().map(|(size, count)| (size, *count))).copied()
}

/// Accumulate sizes of rendered text fragments that belong to the footnote body
/// `haystack`. A fragment must be ≥3 chars and a substring of the footnote text, so
/// single-glyph markers and unrelated body text do not register.
fn collect_footnote_text_sizes(frame: &Frame, haystack: &str, out: &mut HashMap<u32, usize>) {
    super::frames::visit_frame_items(frame, false, &mut |_, item| {
        let FrameItem::Text(text) = item else { return };
        if text.text.chars().filter(|c| !c.is_whitespace()).count() >= MIN_FOOTNOTE_FRAGMENT_CHARS {
            let fragment: String = text.text.chars().filter(|c| !c.is_whitespace()).collect();
            if haystack.contains(fragment.as_str()) {
                *out.entry(pt_to_half_pt(text.size.to_pt())).or_insert(0) += text.glyphs.len();
            }
        }
    });
}

/// Detect heading sizes from rendered text: sizes larger than body, sorted descending.
fn detect_heading_sizes(size_counts: &HashMap<u32, usize>, body_size: u32) -> [u32; 5] {
    let mut larger: Vec<u32> = size_counts
        .keys()
        .copied()
        .filter(|sz| *sz > body_size)
        .collect();
    larger.sort_unstable_by(|a, b| b.cmp(a));
    let mut result = DEFAULT_HEADING_SIZE_OFFSETS.map(|offset| body_size + offset);
    for (i, &sz) in larger.iter().take(5).enumerate() {
        result[i] = sz;
    }
    result
}

/// Detect heading before/after spacing by measuring y-gaps around heading-sized text.
/// Detect heading spacing before/after per level from rendered y-positions.
///
/// For each heading-sized text item, measures the y-gap to its neighbors.
/// Groups by which heading level the size matches, returns per-level arrays.
fn detect_heading_spacing_per_level(
    body_size: u32,
    heading_sizes: &[u32; 5],
    paged: &PagedDocument,
) -> ([u32; 5], [u32; 5]) {
    let default_before = [240; 5];
    let default_after = [120; 5];

    let mut items: Vec<(f64, u32)> = Vec::new();
    for page in paged.pages().iter().take(HEADING_SPACING_SAMPLE_PAGES) {
        collect_y_and_size(&page.frame, &mut items);
    }
    if items.len() < MIN_HEADING_SPACING_ITEMS {
        return (default_before, default_after);
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.dedup_by(|a, b| (a.0 - b.0).abs() < SPACING_Y_DEDUP_TOLERANCE_PT);

    let body_pt = f64::from(body_size) / 2.0;

    // Per-level gap collectors
    let mut before_per_level: [Vec<f64>; 5] = Default::default();
    let mut after_per_level: [Vec<f64>; 5] = Default::default();

    for (i, &(y, sz)) in items.iter().enumerate() {
        // Find which heading level this size matches
        let level = heading_sizes
            .iter()
            .position(|&hs| sz == hs && sz > body_size);
        let Some(level) = level else { continue };

        if i > 0 {
            let gap = y - items[i - 1].0;
            if gap > 0.0 && gap < body_pt * MAX_HEADING_GAP_BODY_MULTIPLE {
                before_per_level[level].push(gap);
            }
        }
        if i + 1 < items.len() {
            let gap = items[i + 1].0 - y;
            if gap > 0.0 && gap < body_pt * MAX_HEADING_GAP_BODY_MULTIPLE {
                after_per_level[level].push(gap);
            }
        }
    }

    let mut result_before = default_before;
    let mut result_after = default_after;

    for level in 0..5 {
        if !before_per_level[level].is_empty() {
            let gaps = &mut before_per_level[level];
            let median = super::stats::median(gaps).unwrap_or(0.0);
            result_before[level] = pt_to_twips(median).min(MAX_HEADING_BEFORE_TWIPS);
        }
        if !after_per_level[level].is_empty() {
            let gaps = &mut after_per_level[level];
            let median = super::stats::median(gaps).unwrap_or(0.0);
            result_after[level] = pt_to_twips(median).min(MAX_HEADING_AFTER_TWIPS);
        }
    }

    (result_before, result_after)
}

/// Detect body paragraph spacing from rendered y-gaps between
/// consecutive body-text lines (excluding heading-sized text).
fn detect_body_paragraph_spacing(
    body_size: u32,
    heading_sizes: &[u32; 5],
    paged: &PagedDocument,
) -> (u32, u32) {
    let mut items: Vec<(f64, u32)> = Vec::new();
    for page in paged.pages().iter().take(BODY_SPACING_SAMPLE_PAGES) {
        collect_y_and_size(&page.frame, &mut items);
    }
    if items.len() < MIN_BODY_SPACING_ITEMS {
        return (0, 0);
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.dedup_by(|a, b| (a.0 - b.0).abs() < SPACING_Y_DEDUP_TOLERANCE_PT);

    let body_pt = f64::from(body_size) / 2.0;
    let heading_min = heading_sizes.iter().copied().min().unwrap_or(body_size + 1);

    // Collect y-gaps between consecutive body-sized items
    let mut body_gaps: Vec<f64> = Vec::new();
    for pair in items.windows(2) {
        let (_, sz0) = pair[0];
        let (_, sz1) = pair[1];
        // Both items must be body-sized (not heading-sized)
        if sz0 < heading_min && sz1 < heading_min {
            let gap = pair[1].0 - pair[0].0;
            // Normal line gaps are ~body_pt * 1.65. Paragraph gaps are larger.
            // Filter to gaps that are plausibly paragraph breaks (> 1.8x body size)
            if gap > body_pt * MIN_PARAGRAPH_GAP_BODY_MULTIPLE
                && gap < body_pt * MAX_PARAGRAPH_GAP_BODY_MULTIPLE
            {
                body_gaps.push(gap);
            }
        }
    }

    if body_gaps.is_empty() {
        return (0, 0);
    }

    let median = super::stats::median(&mut body_gaps).unwrap_or(0.0);

    // The gap includes line height. Paragraph spacing = gap - normal_line_height.
    // Normal line height ≈ body_pt * (1 + leading). We use the median of intra-paragraph
    // line gaps if available, otherwise approximate.
    let mut line_gaps: Vec<f64> = Vec::new();
    for pair in items.windows(2) {
        if pair[0].1 < heading_min && pair[1].1 < heading_min {
            let gap = pair[1].0 - pair[0].0;
            if gap > body_pt * MIN_LINE_PITCH_BODY_MULTIPLE
                && gap <= body_pt * MIN_PARAGRAPH_GAP_BODY_MULTIPLE
            {
                line_gaps.push(gap);
            }
        }
    }

    let normal_line_gap = if line_gaps.is_empty() {
        body_pt * DEFAULT_LINE_PITCH_BODY_MULTIPLE
    } else {
        super::stats::median(&mut line_gaps).unwrap_or(body_pt * DEFAULT_LINE_PITCH_BODY_MULTIPLE)
    };

    let spacing_pt = (median - normal_line_gap).max(0.0);
    let spacing_twips = pt_to_twips(spacing_pt).min(MAX_BODY_SPACING_TWIPS);

    // Use same value for before and after (Typst uses symmetric par spacing)
    (spacing_twips, spacing_twips)
}

fn collect_y_and_size(frame: &Frame, items: &mut Vec<(f64, u32)>) {
    super::frames::visit_frame_items(frame, false, &mut |position, item| {
        if let FrameItem::Text(text) = item {
            items.push((position.y.to_pt(), pt_to_half_pt(text.size.to_pt())));
        }
    });
}

/// Recursively collect font info split by script (ASCII vs CJK), sizes, and y-positions.
fn collect_font_info_split(frame: &Frame, histograms: &mut FontInfoHistograms) {
    super::frames::visit_frame_items(frame, false, &mut |position, item| {
        if let FrameItem::Text(text_item) = item {
            let family = text_item.font.info().family.clone();
            let size_half_pt = pt_to_half_pt(text_item.size.to_pt());
            let glyph_count = text_item.glyphs.len();
            *histograms.size_counts.entry(size_half_pt).or_insert(0) += glyph_count;
            histograms
                .y_positions
                .push((position.y.to_pt(), size_half_pt));
            let cap_h = text_item.font.metrics().cap_height.get();
            histograms.body_cap_heights.push(cap_h);

            let has_cjk = text_item.text.chars().any(is_cjk_char);
            let has_ascii = text_item.text.chars().any(|c| c.is_ascii_alphabetic());
            if has_cjk {
                *histograms.cjk_fonts.entry(family.clone()).or_insert(0) += glyph_count;
            }
            if has_ascii || !has_cjk {
                *histograms.ascii_fonts.entry(family).or_insert(0) += glyph_count;
            }
        }
    });
}
