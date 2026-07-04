//! `PagedDocument` -> page settings, document style, font detection.
//!
//! Extracts the most-common font family / size from rendered frames
//! and computes page dimensions + margins from the first page.
//! Also detects section breaks, headers/footers, and column layouts.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use typort_ooxml::document::{
    Alignment, Document, DocumentStyle, FootnoteFormat, HeaderFooter, InlineElement,
    PageNumberFormat, PageSettings, Paragraph, ParagraphStyle, Run, SectionBreak, SectionBreakType,
};
use typst::World;
use typst::layout::{Frame, FrameItem, Point};
use typst_layout::PagedDocument;

/// Per-span and per-text style override maps produced by [`build_style_override_maps`].
type StyleOverrideMaps = (
    HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>>,
    HashMap<String, Vec<RunStyleOverride>>,
);

/// Extract document style (fonts, sizes, spacing) from the rendered `PagedDocument`.
///
/// Walks the first few pages' frames to find the most common font family and size,
/// which represent the body text styling.
#[must_use]
pub fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
    let mut ascii_font_counts: HashMap<String, usize> = HashMap::new();
    let mut cjk_font_counts: HashMap<String, usize> = HashMap::new();
    let mut size_counts: HashMap<u32, usize> = HashMap::new();
    let mut y_positions: Vec<(f64, u32)> = Vec::new();
    let mut body_cap_heights: Vec<f64> = Vec::new();

    for page in paged.pages().iter().take(3) {
        collect_font_info_split(
            &page.frame,
            Point::zero(),
            &mut ascii_font_counts,
            &mut cjk_font_counts,
            &mut size_counts,
            &mut y_positions,
            &mut body_cap_heights,
        );
    }

    // Detect body font (most common per script). On a count tie, fall back to
    // the alphabetically-first name so the choice is deterministic across runs
    // (a bare `max_by_key` on count would pick whichever the HashMap happened to
    // iterate first).
    let body_font_ascii = ascii_font_counts
        .iter()
        .max_by_key(|(f, c)| (**c, std::cmp::Reverse((**f).clone())))
        .map_or_else(|| "Times New Roman".to_string(), |(f, _)| f.clone());

    let body_font_east_asia = cjk_font_counts
        .iter()
        .max_by_key(|(f, c)| (**c, std::cmp::Reverse((**f).clone())))
        .map_or_else(|| body_font_ascii.clone(), |(f, _)| f.clone());

    // Detect body size (most common). On a count tie, prefer the smaller size:
    // body text is the baseline, and emphasis/headings are larger. The
    // `Reverse(size)` tie-break also makes the result deterministic — without it
    // a single heading line tying the body line (e.g. a one-line document) would
    // flip the detected body size between runs (HashMap iteration order).
    let body_size_half_pt = size_counts
        .iter()
        .max_by_key(|(size, count)| (**count, std::cmp::Reverse(**size)))
        .map_or(21, |(size, _)| *size);

    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = detect_first_line_indent(paged, body_pt);
    let line_spacing = detect_line_spacing(&y_positions, body_size_half_pt);

    // Determine body font's cap-height ratio from collected metrics.
    // Cap-height is the text box height Typst uses for line layout.
    let body_cap_height_ratio = if body_cap_heights.is_empty() {
        0.66
    } else {
        let mut sorted = body_cap_heights.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    };

    // Detect code font (monospace font that isn't the body font)
    let code_font = ascii_font_counts
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
        .max_by_key(|(f, c)| (**c, std::cmp::Reverse((**f).clone())))
        .map_or_else(|| "Courier New".to_string(), |(f, _)| f.clone());

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

pub(crate) fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' |
        '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}' |
        '\u{AC00}'..='\u{D7AF}' | '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}' |
        '\u{FE30}'..='\u{FE4F}' |
        '\u{2E80}'..='\u{2EFF}' | '\u{2F00}'..='\u{2FDF}' |
        '\u{20000}'..='\u{2A6DF}'
    )
}

fn detect_first_line_indent(paged: &PagedDocument, body_pt: f64) -> u32 {
    let Some(page) = paged.pages().first() else {
        return pt_to_twips(body_pt * 2.0);
    };

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, Point::zero(), &mut fragments);

    if fragments.len() < 4 {
        return pt_to_twips(body_pt * 2.0);
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

    if left_edges.len() < 2 {
        return pt_to_twips(body_pt * 2.0);
    }

    // Find the two most common left-edge positions (body margin + indented margin)
    let mut x_clusters: Vec<(f64, usize)> = Vec::new();
    for &x in &left_edges {
        if let Some(c) = x_clusters.iter_mut().find(|(cx, _)| (x - *cx).abs() < 3.0) {
            c.1 += 1;
        } else {
            x_clusters.push((x, 1));
        }
    }
    x_clusters.sort_by_key(|b| std::cmp::Reverse(b.1));

    if x_clusters.len() >= 2 {
        let margin_x = x_clusters[0].0.min(x_clusters[1].0);
        let indent_x = x_clusters[0].0.max(x_clusters[1].0);
        let indent_pt = indent_x - margin_x;
        if indent_pt > 1.0 && indent_pt < body_pt * 6.0 {
            return pt_to_twips(indent_pt);
        }
    }

    // Fallback: 2 chars wide
    pt_to_twips(body_pt * 2.0)
}

fn detect_line_spacing(y_positions: &[(f64, u32)], body_size_half_pt: u32) -> u32 {
    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let default_twips = pt_to_twips(body_pt * 1.65);

    // Filter to only body-sized text items (within ±1 half-point of detected body size)
    let mut body_ys: Vec<f64> = y_positions
        .iter()
        .filter(|(_, sz)| sz.abs_diff(body_size_half_pt) <= 1)
        .map(|(y, _)| *y)
        .collect();

    if body_ys.len() < 2 {
        return default_twips;
    }
    body_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    body_ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);

    let mut gaps: Vec<f64> = Vec::new();
    for pair in body_ys.windows(2) {
        let gap = pair[1] - pair[0];
        if gap > body_pt * 0.8 && gap < body_pt * 3.0 {
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
    let mode_key = gap_counts
        .into_iter()
        .max_by_key(|(key, count)| (*count, std::cmp::Reverse(*key)))
        .map_or(0, |(key, _)| key);
    let mode_pitch = f64::from(mode_key) / 2.0;
    let spacing = pt_to_twips(mode_pitch);
    spacing.clamp(160, 960)
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

    for page in paged.pages().iter().take(3) {
        let page_width = page.frame.width().to_pt();
        let page_height = page.frame.height().to_pt();
        // Default-margin body zone (see detect_first_line_indent): style-time
        // heuristic only, overridden by `#set par(justify:)` when declared.
        let (body_top, body_bottom) = find_body_zone(page_width, page_height, None, None);
        collect_right_edges(
            &page.frame,
            Point::zero(),
            body_top,
            body_bottom,
            &mut line_items,
        );
    }

    if line_items.len() < 4 {
        // Not enough data to decide; Typst default is left-aligned
        return "left".to_string();
    }

    // Group by y-position (same line) and compute the max right edge per line.
    line_items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut line_right_edges: Vec<f64> = Vec::new();
    let mut current_y = line_items[0].0;
    let mut current_max_x = line_items[0].1;
    let y_tolerance = 2.0; // items within 2pt are on the same line

    for &(y, right_x) in &line_items[1..] {
        if (y - current_y).abs() <= y_tolerance {
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

    if line_right_edges.len() < 3 {
        return "left".to_string();
    }

    // Exclude the last line of each paragraph — it's typically shorter.
    // We detect paragraph-final lines as lines whose right edge is significantly
    // shorter than the median right edge.
    line_right_edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_right = line_right_edges[line_right_edges.len() / 2];

    // Keep only lines whose right edge is within 80% of the median (full lines).
    let full_lines: Vec<f64> = line_right_edges
        .iter()
        .copied()
        .filter(|&x| x >= median_right * 0.85)
        .collect();

    if full_lines.len() < 3 {
        return "left".to_string();
    }

    // Compute standard deviation of full-line right edges.
    #[allow(clippy::cast_precision_loss)]
    let n = full_lines.len() as f64;
    let mean: f64 = full_lines.iter().sum::<f64>() / n;
    let variance: f64 = full_lines.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    // Justified text: right edges are very uniform (std_dev < 2pt).
    // Ragged text: right edges vary by many points (std_dev > 5pt typically).
    if std_dev < 3.0 {
        "both".to_string()
    } else {
        "left".to_string()
    }
}

/// Collect (y, `right_edge_x`) pairs for text items in the body zone.
fn collect_right_edges(
    frame: &Frame,
    offset: Point,
    body_top: f64,
    body_bottom: f64,
    items: &mut Vec<(f64, f64)>,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let y = abs_y.to_pt();
                if y >= body_top && y <= body_bottom {
                    let right_edge = abs_x.to_pt() + text_item.width().to_pt();
                    items.push((y, right_edge));
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_right_edges(&group.frame, new_offset, body_top, body_bottom, items);
            }
            _ => {}
        }
    }
}

/// Detect code block font size: the most common size smaller than body that's used with mono fonts.
/// Falls back to `body_size - 3` half-points.
fn detect_code_size(size_counts: &HashMap<u32, usize>, body_size: u32) -> u32 {
    size_counts
        .iter()
        .filter(|(sz, _)| **sz < body_size && **sz >= 12)
        .max_by_key(|(sz, c)| (**c, std::cmp::Reverse(**sz)))
        .map_or(body_size.saturating_sub(3).max(14), |(sz, _)| *sz)
}

/// Fallback footnote text size from the global histogram: the smallest size with
/// significant usage. This is imprecise — it cannot tell the footnote body text
/// from the (smaller) superscript reference marker — so it is only used when the
/// semantic [`detect_footnote_text_size`] finds no footnote text to measure.
fn detect_footnote_size(size_counts: &HashMap<u32, usize>, body_size: u32) -> u32 {
    size_counts
        .iter()
        .filter(|(sz, count)| **sz < body_size && **count >= 3 && **sz >= 12)
        .min_by_key(|(sz, _)| *sz)
        .map_or(body_size.saturating_sub(3).max(14), |(sz, _)| *sz)
}

/// Refine `doc.style.footnote_size_half_pt` from the actual footnote entries when a
/// Paged render is available — overriding the imprecise global-histogram fallback
/// with the footnote body size measured by [`detect_footnote_text_size`].
pub(super) fn apply_footnote_text_size(
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
    if haystack.chars().count() < 4 {
        return None;
    }
    let mut size_counts: HashMap<u32, usize> = HashMap::new();
    for page in paged.pages() {
        collect_footnote_text_sizes(&page.frame, &haystack, &mut size_counts);
    }
    // The footnote body is the dominant matched size; a tie prefers the smaller
    // size (footnotes are smaller than any body text that coincidentally matches).
    size_counts
        .into_iter()
        .max_by_key(|(sz, count)| (*count, std::cmp::Reverse(*sz)))
        .map(|(sz, _)| sz)
}

/// Accumulate sizes of rendered text fragments that belong to the footnote body
/// `haystack`. A fragment must be ≥3 chars and a substring of the footnote text, so
/// single-glyph markers and unrelated body text do not register.
fn collect_footnote_text_sizes(frame: &Frame, haystack: &str, out: &mut HashMap<u32, usize>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Text(t) => {
                // Gate on length BEFORE allocating the filtered copy — most
                // runs on a page are short shaped fragments, and this walk
                // visits every text item on every page.
                if t.text.chars().filter(|c| !c.is_whitespace()).count() >= 3 {
                    let frag: String = t.text.chars().filter(|c| !c.is_whitespace()).collect();
                    if haystack.contains(frag.as_str()) {
                        *out.entry(pt_to_half_pt(t.size.to_pt())).or_insert(0) += t.glyphs.len();
                    }
                }
            }
            FrameItem::Group(group) => collect_footnote_text_sizes(&group.frame, haystack, out),
            _ => {}
        }
    }
}

/// Detect heading sizes from rendered text: sizes larger than body, sorted descending.
fn detect_heading_sizes(size_counts: &HashMap<u32, usize>, body_size: u32) -> [u32; 5] {
    let mut larger: Vec<u32> = size_counts
        .keys()
        .copied()
        .filter(|sz| *sz > body_size)
        .collect();
    larger.sort_unstable_by(|a, b| b.cmp(a));
    let mut result = [
        body_size + 9,
        body_size + 7,
        body_size + 5,
        body_size + 3,
        body_size + 1,
    ];
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
    for page in paged.pages().iter().take(5) {
        collect_y_and_size(&page.frame, Point::zero(), &mut items);
    }
    if items.len() < 3 {
        return (default_before, default_after);
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);

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
            if gap > 0.0 && gap < body_pt * 15.0 {
                before_per_level[level].push(gap);
            }
        }
        if i + 1 < items.len() {
            let gap = items[i + 1].0 - y;
            if gap > 0.0 && gap < body_pt * 15.0 {
                after_per_level[level].push(gap);
            }
        }
    }

    let mut result_before = default_before;
    let mut result_after = default_after;

    for level in 0..5 {
        if !before_per_level[level].is_empty() {
            let gaps = &mut before_per_level[level];
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = gaps[gaps.len() / 2];
            result_before[level] = pt_to_twips(median).min(1500);
        }
        if !after_per_level[level].is_empty() {
            let gaps = &mut after_per_level[level];
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = gaps[gaps.len() / 2];
            result_after[level] = pt_to_twips(median).min(800);
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
    for page in paged.pages().iter().take(3) {
        collect_y_and_size(&page.frame, Point::zero(), &mut items);
    }
    if items.len() < 4 {
        return (0, 0);
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);

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
            if gap > body_pt * 1.8 && gap < body_pt * 8.0 {
                body_gaps.push(gap);
            }
        }
    }

    if body_gaps.is_empty() {
        return (0, 0);
    }

    body_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = body_gaps[body_gaps.len() / 2];

    // The gap includes line height. Paragraph spacing = gap - normal_line_height.
    // Normal line height ≈ body_pt * (1 + leading). We use the median of intra-paragraph
    // line gaps if available, otherwise approximate.
    let mut line_gaps: Vec<f64> = Vec::new();
    for pair in items.windows(2) {
        if pair[0].1 < heading_min && pair[1].1 < heading_min {
            let gap = pair[1].0 - pair[0].0;
            if gap > body_pt * 0.8 && gap <= body_pt * 1.8 {
                line_gaps.push(gap);
            }
        }
    }

    let normal_line_gap = if line_gaps.is_empty() {
        body_pt * 1.65
    } else {
        line_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        line_gaps[line_gaps.len() / 2]
    };

    let spacing_pt = (median - normal_line_gap).max(0.0);
    let spacing_twips = pt_to_twips(spacing_pt).min(1000);

    // Use same value for before and after (Typst uses symmetric par spacing)
    (spacing_twips, spacing_twips)
}

fn collect_y_and_size(frame: &Frame, offset: Point, items: &mut Vec<(f64, u32)>) {
    for (pos, item) in frame.items() {
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let size_half_pt = pt_to_half_pt(text_item.size.to_pt());
                items.push((abs_y.to_pt(), size_half_pt));
            }
            FrameItem::Group(group) => {
                collect_y_and_size(&group.frame, Point::new(offset.x + pos.x, abs_y), items);
            }
            _ => {}
        }
    }
}

/// Recursively collect font info split by script (ASCII vs CJK), sizes, and y-positions.
fn collect_font_info_split(
    frame: &Frame,
    offset: Point,
    ascii_fonts: &mut HashMap<String, usize>,
    cjk_fonts: &mut HashMap<String, usize>,
    size_counts: &mut HashMap<u32, usize>,
    y_positions: &mut Vec<(f64, u32)>,
    body_cap_heights: &mut Vec<f64>,
) {
    for (pos, item) in frame.items() {
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let family = text_item.font.info().family.clone();
                let size_half_pt = pt_to_half_pt(text_item.size.to_pt());
                let glyph_count = text_item.glyphs.len();
                *size_counts.entry(size_half_pt).or_insert(0) += glyph_count;
                y_positions.push((abs_y.to_pt(), size_half_pt));
                let cap_h = text_item.font.metrics().cap_height.get();
                body_cap_heights.push(cap_h);

                let has_cjk = text_item.text.chars().any(is_cjk_char);
                let has_ascii = text_item.text.chars().any(|c| c.is_ascii_alphabetic());
                if has_cjk {
                    *cjk_fonts.entry(family.clone()).or_insert(0) += glyph_count;
                }
                if has_ascii || !has_cjk {
                    *ascii_fonts.entry(family).or_insert(0) += glyph_count;
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(offset.x + pos.x, abs_y);
                collect_font_info_split(
                    &group.frame,
                    new_offset,
                    ascii_fonts,
                    cjk_fonts,
                    size_counts,
                    y_positions,
                    body_cap_heights,
                );
            }
            _ => {}
        }
    }
}

/// Extract page dimensions from the `PagedDocument` and apply to `PageSettings`.
pub fn extract_page_settings(paged: &PagedDocument, settings: &mut PageSettings) {
    let Some(page) = paged.pages().first() else {
        return;
    };
    let w = page.frame.width().to_pt();
    let h = page.frame.height().to_pt();
    settings.width_twips = pt_to_twips(w);
    settings.height_twips = pt_to_twips(h);

    let default_margin = default_margin_pt(w, h);
    let default_twips = pt_to_twips(default_margin);
    settings.margin_top = default_twips;
    settings.margin_bottom = default_twips;
    settings.margin_left = default_twips;
    settings.margin_right = default_twips;
}

// ---------------------------------------------------------------------------
// Source AST extraction for page/text/par settings
// ---------------------------------------------------------------------------

/// All style overrides extracted from `#set` rules in the source AST.
/// Each field is `None` if the source doesn't set it (use heuristic fallback).
#[derive(Default)]
pub struct SourceStyleOverrides {
    // #set page(margin: ...)
    pub margin_top: Option<u32>,
    pub margin_bottom: Option<u32>,
    pub margin_left: Option<u32>,
    pub margin_right: Option<u32>,
    // #set page(columns: N)
    pub columns: Option<u32>,
    // #set page(numbering: "1"/"i"/...)
    pub page_numbering: Option<String>,
    // #set text(font: ..., size: ...)
    pub text_font: Option<Vec<String>>,
    pub text_size_half_pt: Option<u32>,
    // #set text(lang: "zh", region: "cn") — ISO 639 lang + optional ISO 3166 region.
    pub text_lang: Option<String>,
    pub text_region: Option<String>,
    // #set par(first-line-indent: ..., leading: ..., justify: ..., spacing: ...)
    // Values in twips for absolute units, or as em*1000 (milliem) for em units.
    pub first_line_indent_twips: Option<u32>,
    pub first_line_indent_em: Option<f64>,
    // #set par(first-line-indent: (amount: ..., all: true)) — indent every
    // paragraph, including the first after a heading.
    pub first_line_indent_all: Option<bool>,
    pub par_leading_twips: Option<u32>,
    pub par_leading_em: Option<f64>,
    pub par_spacing_twips: Option<u32>,
    pub par_spacing_em: Option<f64>,
    pub justify: Option<bool>,
}

impl SourceStyleOverrides {
    /// Fill any `None` fields from `other` (used to merge imported file overrides).
    pub fn merge_from(&mut self, other: &SourceStyleOverrides) {
        macro_rules! fill {
            ($field:ident) => {
                if self.$field.is_none() {
                    self.$field = other.$field.clone();
                }
            };
        }
        fill!(margin_top);
        fill!(margin_bottom);
        fill!(margin_left);
        fill!(margin_right);
        fill!(columns);
        fill!(page_numbering);
        fill!(text_font);
        fill!(text_size_half_pt);
        fill!(text_lang);
        fill!(text_region);
        fill!(first_line_indent_twips);
        fill!(first_line_indent_em);
        fill!(first_line_indent_all);
        fill!(par_leading_twips);
        fill!(par_leading_em);
        fill!(par_spacing_twips);
        fill!(par_spacing_em);
        fill!(justify);
    }
}

/// Extract style overrides from Typst source AST in a single walk.
///
/// Reads `#set page(...)`, `#set text(...)`, `#set par(...)` rules, but only
/// those in *document-global* scope: top-level of the file, or inside the
/// closure named by a `#show:` template (whose names are passed in
/// `template_names`). A `set text(size:)` buried in a `#block[…]` or a
/// non-template helper closure is element-/locally-scoped and must not clobber
/// the real global body size.
#[must_use]
pub fn extract_source_style_overrides(
    source: &str,
    template_names: &[String],
) -> SourceStyleOverrides {
    let root = typst_syntax::parse(source);
    let mut ovr = SourceStyleOverrides::default();
    collect_global_set_rules(&root, template_names, true, &mut ovr);
    ovr
}

/// Parse `source` and return the names of the closures applied as document
/// templates via `#show: …` (e.g. `tmpl` for `#show: tmpl.with(...)`).
#[must_use]
pub fn extract_show_template_names_from_source(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);
    extract_show_template_names(&root)
}

/// Collect the closure names referenced by document-wide `#show: NAME` /
/// `#show: NAME.with(...)` rules (`selector()` is `None`). The named closure
/// holds the document's real global `set` rules, so its body must be treated as
/// global scope during [`collect_global_set_rules`].
fn extract_show_template_names(root: &typst_syntax::SyntaxNode) -> Vec<String> {
    use typst_syntax::SyntaxKind;
    use typst_syntax::ast;

    let mut names = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        if node.kind() == SyntaxKind::ShowRule
            && let Some(show) = node.cast::<ast::ShowRule<'_>>()
            && show.selector().is_none()
        {
            match show.transform() {
                // `#show: tmpl.with(...)` => FuncCall whose callee is a
                // FieldAccess `tmpl.with`; the template is the FieldAccess target.
                ast::Expr::FuncCall(call) => {
                    if let ast::Expr::FieldAccess(access) = call.callee()
                        && access.field().as_str() == "with"
                        && let ast::Expr::Ident(ident) = access.target()
                    {
                        names.push(ident.as_str().to_string());
                    } else if let ast::Expr::Ident(ident) = call.callee() {
                        // `#show: tmpl()` => bare ident callee.
                        names.push(ident.as_str().to_string());
                    }
                }
                // `#show: tmpl` => the transform is the ident itself.
                ast::Expr::Ident(ident) => names.push(ident.as_str().to_string()),
                _ => {}
            }
        }
        for child in node.children() {
            stack.push(child.clone());
        }
    }
    names
}

/// A `#set par(hanging-indent: …)` rule recovered from the source: the byte
/// offset at which it takes effect, and whether it sets a non-zero indent (vs a
/// reset to `0pt`/`0em`).
pub struct ParHangingRule {
    pub offset: usize,
    pub nonzero: bool,
}

/// Collect every `#set par(hanging-indent: …)` rule from the source, in document
/// order, with the byte offset at which it takes effect.
///
/// Unlike [`extract_source_style_overrides`] (which collapses to a single
/// document-wide value), a hanging-indent set-rule applies from its position
/// onward, so each occurrence *and* its byte position matter — a rule before a
/// hand-written reference list governs only the paragraphs that follow it. This
/// honors a value the author literally declared; it is not genre/keyword
/// matching. Spans are read off the real `Source` tree so byte offsets line up
/// with the run spans used to locate each paragraph.
#[must_use]
pub fn collect_par_hanging_indent_rules(source: &typst_syntax::Source) -> Vec<ParHangingRule> {
    let mut rules = Vec::new();
    collect_hanging_rules(source, source.root(), &mut rules);
    rules.sort_by_key(|r| r.offset);
    rules
}

/// Byte range of `span` within `source`.
///
/// typst 0.15 changed `Source::range` to take a decomposed
/// `(SpanNumber, Option<SubRange>)` instead of a `Span`; this performs that
/// decomposition (the same one `WorldExt::range` does) for a span that points
/// into `source`. Returns `None` for a detached span or one in another file.
fn span_range_in_source(
    source: &typst_syntax::Source,
    span: typst_syntax::Span,
) -> Option<std::ops::Range<usize>> {
    use typst_syntax::{DiagSpan, DiagSpanKind};
    match DiagSpan::from(span).get() {
        DiagSpanKind::Number { id, num, sub_range } if id == source.id() => {
            source.range(num, sub_range)
        }
        DiagSpanKind::Range { id, range } if id == source.id() => Some(range),
        _ => None,
    }
}

fn collect_hanging_rules(
    source: &typst_syntax::Source,
    node: &typst_syntax::SyntaxNode,
    out: &mut Vec<ParHangingRule>,
) {
    use typst_syntax::SyntaxKind;

    // Set-rules nested inside a show-rule are element-scoped, not global.
    if node.kind() == SyntaxKind::ShowRule {
        return;
    }

    if node.kind() == SyntaxKind::SetRule
        && let Some(set) = node.cast::<typst_syntax::ast::SetRule<'_>>()
        && matches!(set.target(), typst_syntax::ast::Expr::Ident(i) if i.as_str() == "par")
    {
        for arg in set.args().items() {
            if let typst_syntax::ast::Arg::Named(named) = arg
                && named.name().as_str() == "hanging-indent"
                && let typst_syntax::ast::Expr::Numeric(n) = named.expr()
                && let Some(range) = span_range_in_source(source, node.span())
            {
                let (value, _unit) = n.get();
                out.push(ParHangingRule {
                    offset: range.start,
                    nonzero: value.abs() > f64::EPSILON,
                });
            }
        }
    }

    for child in node.children() {
        collect_hanging_rules(source, child, out);
    }
}

/// Extract file paths from `#import "path"` statements in Typst source.
///
/// Returns relative paths as written in the source (e.g. `"lib.typ"`).
#[must_use]
pub fn extract_import_paths(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);
    let mut paths = Vec::new();
    collect_import_paths(&root, &mut paths);
    paths
}

fn collect_import_paths(node: &typst_syntax::SyntaxNode, paths: &mut Vec<String>) {
    use typst_syntax::SyntaxKind;
    if node.kind() == SyntaxKind::ModuleImport
        && let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
        && let typst_syntax::ast::Expr::Str(s) = import.source()
    {
        paths.push(s.get().to_string());
    }
    for child in node.children() {
        collect_import_paths(child, paths);
    }
}

/// String-literal `#import`/`#include` targets of a parsed source (one level).
fn collect_module_paths(node: &typst_syntax::SyntaxNode, paths: &mut Vec<String>) {
    use typst_syntax::SyntaxKind;
    match node.kind() {
        SyntaxKind::ModuleImport => {
            if let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
                && let typst_syntax::ast::Expr::Str(s) = import.source()
            {
                paths.push(s.get().to_string());
            }
        }
        SyntaxKind::ModuleInclude => {
            if let Some(include) = node.cast::<typst_syntax::ast::ModuleInclude<'_>>()
                && let typst_syntax::ast::Expr::Str(s) = include.source()
            {
                paths.push(s.get().to_string());
            }
        }
        _ => {}
    }
    for child in node.children() {
        collect_module_paths(child, paths);
    }
}

/// The text of every local source file reachable from the main source through
/// string-literal `#import`/`#include` chains, the main text itself first.
/// Package imports (`@preview/...`) are skipped, cycles are visited once, and
/// paths resolve the way Typst resolves them: relative to the importing file's
/// directory, or to `root` when they start with `/`. Consumers that scan the
/// AST for a declaration (e.g. a template-drawn `#line()`) must scan ALL of
/// these — a declaration living in an imported template is just as
/// authoritative as one in the main file.
#[must_use]
pub fn collect_reachable_source_texts(
    root: &Path,
    main_dir: &Path,
    main_text: &str,
) -> Vec<String> {
    let mut texts = vec![main_text.to_string()];
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(main_dir.to_path_buf(), main_text.to_string())];

    while let Some((dir, text)) = stack.pop() {
        let mut paths = Vec::new();
        collect_module_paths(&typst_syntax::parse(&text), &mut paths);
        for path in paths {
            if path.starts_with('@') {
                continue; // package import — not a local file
            }
            let resolved = if let Some(rooted) = path.strip_prefix('/') {
                root.join(rooted)
            } else {
                dir.join(&path)
            };
            let canonical = resolved.canonicalize().unwrap_or(resolved);
            if !visited.insert(canonical.clone()) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&canonical) {
                let next_dir = canonical
                    .parent()
                    .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
                texts.push(content.clone());
                stack.push((next_dir, content));
            }
        }
    }
    texts
}

/// Walk the AST collecting only document-global `set`/`#page(...)` rules into
/// `ovr`. `in_global_scope` tracks whether the current node is at file top-level
/// or inside a `#show:` template closure body; set rules nested in content
/// blocks or helper closures are skipped (still recursed for nested templates).
fn collect_global_set_rules(
    node: &typst_syntax::SyntaxNode,
    template_names: &[String],
    in_global_scope: bool,
    ovr: &mut SourceStyleOverrides,
) {
    use typst_syntax::SyntaxKind;
    use typst_syntax::ast;

    match node.kind() {
        // A show rule's recipe applies to specific elements, not globally — and
        // its own `set` rules are element-scoped. Don't descend.
        SyntaxKind::ShowRule => return,
        // A content block (`[...]` / `#block[...]`) introduces a local scope.
        SyntaxKind::ContentBlock => {
            for child in node.children() {
                collect_global_set_rules(child, template_names, false, ovr);
            }
            return;
        }
        // A `#let NAME(...) = …` closure. The show-template closure holds the
        // real document globals; any other closure is a local helper.
        SyntaxKind::LetBinding => {
            if let Some(binding) = node.cast::<ast::LetBinding<'_>>()
                && let ast::LetBindingKind::Closure(name) = binding.kind()
            {
                let child_global = template_names.iter().any(|t| t == name.as_str());
                for child in node.children() {
                    collect_global_set_rules(child, template_names, child_global, ovr);
                }
                return;
            }
        }
        _ => {}
    }

    if in_global_scope {
        if node.kind() == SyntaxKind::SetRule
            && let Some(set) = node.cast::<ast::SetRule<'_>>()
            && let ast::Expr::Ident(ident) = set.target()
        {
            match ident.as_str() {
                "page" => parse_page_args(set.args(), ovr),
                "text" => parse_text_args(set.args(), ovr),
                "par" => parse_par_args(set.args(), ovr),
                _ => {}
            }
        }

        // Also honor the `#page(...)` function-call form (e.g.
        // `#page(columns: 2)[…]`), not just `#set page(...)`. Its named args
        // carry the same page settings.
        if node.kind() == SyntaxKind::FuncCall
            && let Some(call) = node.cast::<ast::FuncCall<'_>>()
            && matches!(call.callee(), ast::Expr::Ident(i) if i.as_str() == "page")
        {
            parse_page_args(call.args(), ovr);
        }
    }

    for child in node.children() {
        collect_global_set_rules(child, template_names, in_global_scope, ovr);
    }
}

fn parse_page_args(args: typst_syntax::ast::Args<'_>, ovr: &mut SourceStyleOverrides) {
    for arg in args.items() {
        let typst_syntax::ast::Arg::Named(named) = arg else {
            continue;
        };
        match named.name().as_str() {
            "margin" => parse_margin_value(named.expr(), ovr),
            "columns" => {
                if let typst_syntax::ast::Expr::Int(i) = named.expr() {
                    ovr.columns = u32::try_from(i.get()).ok();
                }
            }
            "numbering" => {
                if let typst_syntax::ast::Expr::Str(s) = named.expr() {
                    ovr.page_numbering = Some(s.get().to_string());
                }
            }
            _ => {}
        }
    }
}

fn parse_margin_value(expr: typst_syntax::ast::Expr<'_>, ovr: &mut SourceStyleOverrides) {
    match expr {
        typst_syntax::ast::Expr::Numeric(n) => {
            let twips = numeric_to_twips(n);
            if twips > 0 {
                ovr.margin_top = Some(twips);
                ovr.margin_bottom = Some(twips);
                ovr.margin_left = Some(twips);
                ovr.margin_right = Some(twips);
            }
        }
        typst_syntax::ast::Expr::Dict(dict) => {
            // Collect with Typst priority: rest < x/y < individual sides
            let mut rest = None;
            let mut x = None;
            let mut y = None;
            let mut top = None;
            let mut bottom = None;
            let mut left = None;
            let mut right = None;

            for item in dict.items() {
                let typst_syntax::ast::DictItem::Named(entry) = item else {
                    continue;
                };
                if let typst_syntax::ast::Expr::Numeric(n) = entry.expr() {
                    let twips = numeric_to_twips(n);
                    if twips > 0 {
                        match entry.name().as_str() {
                            "rest" => rest = Some(twips),
                            "x" => x = Some(twips),
                            "y" => y = Some(twips),
                            "top" => top = Some(twips),
                            "bottom" => bottom = Some(twips),
                            "left" => left = Some(twips),
                            "right" => right = Some(twips),
                            _ => {}
                        }
                    }
                }
            }

            // Resolve in priority order
            if let Some(v) = rest {
                ovr.margin_top = Some(v);
                ovr.margin_bottom = Some(v);
                ovr.margin_left = Some(v);
                ovr.margin_right = Some(v);
            }
            if let Some(v) = x {
                ovr.margin_left = Some(v);
                ovr.margin_right = Some(v);
            }
            if let Some(v) = y {
                ovr.margin_top = Some(v);
                ovr.margin_bottom = Some(v);
            }
            if let Some(v) = top {
                ovr.margin_top = Some(v);
            }
            if let Some(v) = bottom {
                ovr.margin_bottom = Some(v);
            }
            if let Some(v) = left {
                ovr.margin_left = Some(v);
            }
            if let Some(v) = right {
                ovr.margin_right = Some(v);
            }
        }
        _ => {}
    }
}

fn parse_text_args(args: typst_syntax::ast::Args<'_>, ovr: &mut SourceStyleOverrides) {
    for arg in args.items() {
        match arg {
            typst_syntax::ast::Arg::Named(named) => match named.name().as_str() {
                "font" => {
                    if ovr.text_font.is_none() {
                        ovr.text_font = extract_font_list(named.expr());
                    }
                }
                "size" => {
                    if ovr.text_size_half_pt.is_none()
                        && let typst_syntax::ast::Expr::Numeric(n) = named.expr()
                    {
                        let half_pt = numeric_to_half_pt(n);
                        if half_pt > 0 {
                            ovr.text_size_half_pt = Some(half_pt);
                        }
                    }
                }
                "lang" => {
                    if ovr.text_lang.is_none()
                        && let typst_syntax::ast::Expr::Str(s) = named.expr()
                    {
                        ovr.text_lang = Some(s.get().to_string());
                    }
                }
                "region" => {
                    if ovr.text_region.is_none()
                        && let typst_syntax::ast::Expr::Str(s) = named.expr()
                    {
                        ovr.text_region = Some(s.get().to_string());
                    }
                }
                _ => {}
            },
            typst_syntax::ast::Arg::Pos(typst_syntax::ast::Expr::Numeric(n)) => {
                let half_pt = numeric_to_half_pt(n);
                if half_pt > 0 {
                    ovr.text_size_half_pt = Some(half_pt);
                }
            }
            _ => {}
        }
    }
}

/// Record a first-line-indent amount (em or absolute) onto the overrides.
fn set_first_line_indent_amount(n: typst_syntax::ast::Numeric<'_>, ovr: &mut SourceStyleOverrides) {
    let (value, unit) = n.get();
    if unit == typst_syntax::ast::Unit::Em {
        ovr.first_line_indent_em = Some(value);
    } else {
        ovr.first_line_indent_twips = Some(numeric_to_twips(n));
    }
}

fn parse_par_args(args: typst_syntax::ast::Args<'_>, ovr: &mut SourceStyleOverrides) {
    for arg in args.items() {
        let typst_syntax::ast::Arg::Named(named) = arg else {
            continue;
        };
        match named.name().as_str() {
            "first-line-indent" => {
                if ovr.first_line_indent_twips.is_none() && ovr.first_line_indent_em.is_none() {
                    match named.expr() {
                        typst_syntax::ast::Expr::Numeric(n) => set_first_line_indent_amount(n, ovr),
                        // `(amount: 2em, all: true)`: indent every paragraph,
                        // including the first one after a heading.
                        typst_syntax::ast::Expr::Dict(dict) => {
                            for item in dict.items() {
                                let typst_syntax::ast::DictItem::Named(entry) = item else {
                                    continue;
                                };
                                match entry.name().as_str() {
                                    "amount" => {
                                        if let typst_syntax::ast::Expr::Numeric(n) = entry.expr() {
                                            set_first_line_indent_amount(n, ovr);
                                        }
                                    }
                                    "all" => {
                                        if let typst_syntax::ast::Expr::Bool(b) = entry.expr() {
                                            ovr.first_line_indent_all = Some(b.get());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "leading" => {
                if ovr.par_leading_twips.is_none()
                    && ovr.par_leading_em.is_none()
                    && let typst_syntax::ast::Expr::Numeric(n) = named.expr()
                {
                    let (value, unit) = n.get();
                    if unit == typst_syntax::ast::Unit::Em {
                        ovr.par_leading_em = Some(value);
                    } else {
                        ovr.par_leading_twips = Some(numeric_to_twips(n));
                    }
                }
            }
            "spacing" => {
                if ovr.par_spacing_twips.is_none()
                    && ovr.par_spacing_em.is_none()
                    && let typst_syntax::ast::Expr::Numeric(n) = named.expr()
                {
                    let (value, unit) = n.get();
                    if unit == typst_syntax::ast::Unit::Em {
                        ovr.par_spacing_em = Some(value);
                    } else {
                        ovr.par_spacing_twips = Some(numeric_to_twips(n));
                    }
                }
            }
            "justify" => {
                if ovr.justify.is_none()
                    && let typst_syntax::ast::Expr::Bool(b) = named.expr()
                {
                    ovr.justify = Some(b.get());
                }
            }
            _ => {}
        }
    }
}

fn extract_font_list(expr: typst_syntax::ast::Expr<'_>) -> Option<Vec<String>> {
    match expr {
        typst_syntax::ast::Expr::Str(s) => Some(vec![s.get().to_string()]),
        typst_syntax::ast::Expr::Array(arr) => {
            let fonts: Vec<String> = arr
                .items()
                .filter_map(|item| {
                    if let typst_syntax::ast::ArrayItem::Pos(typst_syntax::ast::Expr::Str(s)) = item
                    {
                        Some(s.get().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if fonts.is_empty() { None } else { Some(fonts) }
        }
        _ => None,
    }
}

/// Convert a length in points to Word twips (1 pt = 20 twips), rounded.
///
/// The single place the `f64 → u32` measurement cast is performed. Negative or
/// out-of-range inputs saturate to a valid `u32` (Rust `as` casts saturate),
/// matching the document model's unsigned twip fields.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn pt_to_twips(pt: f64) -> u32 {
    (pt * 20.0).round().max(0.0) as u32
}

/// Convert a length in points to Word half-points (1 pt = 2 half-pt), rounded.
/// See [`pt_to_twips`] for the cast rationale.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn pt_to_half_pt(pt: f64) -> u32 {
    (pt * 2.0).round().max(0.0) as u32
}

/// Build a BCP-47 language tag (for Word's `w:lang`) from a Typst `lang` code
/// plus an optional `region`, exactly as declared in `#set text(...)`.
///
/// `lang` is an ISO 639 code ("zh", "ja", "de"); `region` is ISO 3166-1
/// ("CN", "JP"). When the region is omitted, fall back to the most common
/// region for that language so Word still gets a fully-qualified tag (Word's
/// East-Asian handling expects one); unknown languages pass through bare.
pub(super) fn lang_region_to_bcp47(lang: &str, region: Option<&str>) -> String {
    let lang = lang.to_ascii_lowercase();
    if let Some(r) = region {
        return format!("{lang}-{}", r.to_ascii_uppercase());
    }
    match lang.as_str() {
        "zh" => "zh-CN".to_string(),
        "ja" => "ja-JP".to_string(),
        "ko" => "ko-KR".to_string(),
        "en" => "en-US".to_string(),
        "de" => "de-DE".to_string(),
        "fr" => "fr-FR".to_string(),
        _ => lang,
    }
}

fn numeric_to_twips(n: typst_syntax::ast::Numeric<'_>) -> u32 {
    pt_to_twips(numeric_to_pt(n))
}

fn numeric_to_half_pt(n: typst_syntax::ast::Numeric<'_>) -> u32 {
    pt_to_half_pt(numeric_to_pt(n))
}

fn numeric_to_pt(n: typst_syntax::ast::Numeric<'_>) -> f64 {
    let (value, unit) = n.get();
    match unit {
        typst_syntax::ast::Unit::Pt => value,
        typst_syntax::ast::Unit::Cm => value * 72.0 / 2.54,
        typst_syntax::ast::Unit::Mm => value * 72.0 / 25.4,
        typst_syntax::ast::Unit::In => value * 72.0,
        typst_syntax::ast::Unit::Em => value * 12.0,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Section break detection
// ---------------------------------------------------------------------------

/// Represents a section detected from page setting changes.
#[derive(Debug)]
pub struct DetectedSection {
    /// 0-based index of the page where this section STARTS (the new section's first page).
    pub start_page: usize,
    /// Page settings for this section.
    pub page_settings: PageSettings,
}

/// Detect section breaks from page setting changes in the `PagedDocument`.
///
/// Returns a list of sections where page settings change. The first element
/// (if any) represents a change starting at `start_page` index. Each section's
/// `page_settings` describes the settings BEFORE the break (i.e., the settings
/// of the section that is ending).
#[must_use]
pub fn detect_section_breaks(paged: &PagedDocument) -> Vec<DetectedSection> {
    if paged.pages().len() < 2 {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let prev_w = paged.pages()[0].frame.width();
    let prev_h = paged.pages()[0].frame.height();
    let mut prev_width = pt_to_twips(prev_w.to_pt());
    let mut prev_height = pt_to_twips(prev_h.to_pt());

    for i in 1..paged.pages().len() {
        let curr_w = pt_to_twips(paged.pages()[i].frame.width().to_pt());
        let curr_h = pt_to_twips(paged.pages()[i].frame.height().to_pt());
        let tol = 20; // 1pt tolerance for rounding
        if curr_w.abs_diff(prev_width) > tol || curr_h.abs_diff(prev_height) > tol {
            let margin = default_margin_pt(
                prev_w.to_pt().min(paged.pages()[i].frame.width().to_pt()),
                prev_h.to_pt().min(paged.pages()[i].frame.height().to_pt()),
            );
            let margin_twips = pt_to_twips(margin);
            sections.push(DetectedSection {
                start_page: i,
                page_settings: PageSettings {
                    width_twips: prev_width,
                    height_twips: prev_height,
                    margin_top: margin_twips,
                    margin_bottom: margin_twips,
                    margin_left: margin_twips,
                    margin_right: margin_twips,
                    columns: None,
                    column_spacing: None,
                },
            });
        }
        prev_width = curr_w;
        prev_height = curr_h;
    }

    sections
}

/// Apply section breaks to the document model.
///
/// Given detected section boundaries from `PagedDocument` and an
/// element → page mapping, place `SectionBreak` on the last paragraph
/// before each section boundary.
///
/// `element_page_map` maps each element index to its 1-based page number.
/// If the map is empty, falls back to proportional mapping.
pub fn apply_section_breaks(
    doc: &mut typort_ooxml::document::Document,
    sections: &[DetectedSection],
    element_page_map: &[usize],
) {
    if sections.is_empty() {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    for section in sections {
        // Find the last element on the page before the section break.
        // section.start_page is the 0-based index of the new section's
        // first page, so we look for the last element on page `start_page`
        // (1-based) — i.e., the element just before the new section begins.
        let target_page = section.start_page; // 0-based → 1-based = start_page itself

        let approx_idx = if !element_page_map.is_empty() && element_page_map.len() == total_elements
        {
            // Use the introspector-based mapping: find the last element
            // whose page number is < start_page+1 (i.e., on a page before
            // the new section).
            element_page_map
                .iter()
                .enumerate()
                .rev()
                .find(|(_, page)| **page <= target_page)
                .map_or(0, |(idx, _)| idx)
        } else {
            // Fallback: proportional mapping (legacy behaviour)
            let total_pages = element_page_map
                .len()
                .max(sections.last().map_or(1, |s| s.start_page + 1));
            section.start_page * total_elements / total_pages.max(1)
        };

        // Find the nearest paragraph at or before `approx_idx`
        let para_idx = find_nearest_paragraph(&doc.body.elements, approx_idx);
        if let Some(idx) = para_idx
            && let typort_ooxml::document::BlockElement::Paragraph(para) =
                &mut doc.body.elements[idx]
        {
            para.section_break = Some(SectionBreak {
                break_type: SectionBreakType::NextPage,
                page_settings: Some(section.page_settings.clone()),
            });
        }
    }
}

/// Find the nearest paragraph at or before the given index.
fn find_nearest_paragraph(
    elements: &[typort_ooxml::document::BlockElement],
    target: usize,
) -> Option<usize> {
    let target = target.min(elements.len().saturating_sub(1));
    // Search backwards from target for a paragraph
    for i in (0..=target).rev() {
        if matches!(
            elements[i],
            typort_ooxml::document::BlockElement::Paragraph(_)
        ) {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Header/Footer extraction
// ---------------------------------------------------------------------------

/// A text fragment extracted from a page frame, with position info.
struct TextFragment {
    y: f64,
    x: f64,
    text: String,
}

/// Recursively collect text fragments with absolute positions from a frame.
fn collect_text_fragments(frame: &Frame, offset: Point, items: &mut Vec<TextFragment>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if !text.is_empty() {
                    items.push(TextFragment {
                        y: abs_y.to_pt(),
                        x: abs_x.to_pt(),
                        text,
                    });
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_text_fragments(&group.frame, new_offset, items);
            }
            _ => {}
        }
    }
}

/// Compute the default Typst margin for a page in pt.
///
/// Typst uses `2.5/21 * min(width, height)` as the default margin.
fn default_margin_pt(page_width: f64, page_height: f64) -> f64 {
    let smaller = page_width.min(page_height);
    2.5 / 21.0 * smaller
}

/// Identify the body content zone using margin-based boundaries.
///
/// Resolved top/bottom page margins in pt — the same values the docx writes to
/// `w:pgMar` (paged default overridden by `#set page(margin:)` from the source
/// AST). Body-zone consumers take this so their header/footer boundary tracks
/// the document's actual margins: with author margins smaller than Typst's
/// default, using the default boundary silently drops real body content.
#[derive(Clone, Copy)]
pub struct MarginsPt {
    pub top: f64,
    pub bottom: f64,
}

impl MarginsPt {
    /// Read the resolved margins from the document's `PageSettings`.
    #[must_use]
    pub fn from_settings(settings: &PageSettings) -> Self {
        Self {
            top: f64::from(settings.margin_top) / 20.0,
            bottom: f64::from(settings.margin_bottom) / 20.0,
        }
    }
}

/// Typst renders headers in the top margin area and footers in the
/// bottom margin area. We use the actual margins as the boundary
/// to separate these zones.
///
/// `margin_top_pt` and `margin_bottom_pt`: actual margins in pt. If `None`,
/// falls back to Typst's default margin (`2.5/21 * min(w, h)`).
///
/// Returns `(body_top, body_bottom)` in pt — the y-range of the body zone.
pub(super) fn find_body_zone(
    page_width: f64,
    page_height: f64,
    margin_top_pt: Option<f64>,
    margin_bottom_pt: Option<f64>,
) -> (f64, f64) {
    let default_margin = default_margin_pt(page_width, page_height);
    let mt = margin_top_pt.unwrap_or(default_margin);
    let mb = margin_bottom_pt.unwrap_or(default_margin);
    // Body starts at the top margin line and ends at the bottom margin line.
    // Headers are positioned at margin * (1 - header_ascent) where header_ascent
    // defaults to 0.3, so header text is at ~margin * 0.7 from top.
    // Use margin * 0.9 as the boundary to safely include all header content
    // above the body zone.
    let body_top = mt * 0.9;
    let body_bottom = page_height - mb * 0.9;
    (body_top, body_bottom)
}

/// Extract header content from the top margin area of the first page.
#[must_use]
pub fn extract_header(paged: &PagedDocument, margins: MarginsPt) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Top, margins)
}

/// Extract footer content from the bottom margin area of the first page.
#[must_use]
pub fn extract_footer(paged: &PagedDocument, margins: MarginsPt) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Bottom, margins)
}

enum MarginZone {
    Top,
    Bottom,
}

#[allow(clippy::needless_pass_by_value)]
fn extract_margin_zone(
    paged: &PagedDocument,
    zone: MarginZone,
    margins: MarginsPt,
) -> Option<HeaderFooter> {
    let page = paged.pages().first()?;
    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, Point::zero(), &mut fragments);

    let (body_top, body_bottom) = find_body_zone(
        page_width,
        page_height,
        Some(margins.top),
        Some(margins.bottom),
    );

    let mut items: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| match zone {
            MarginZone::Top => f.y < body_top && f.y > 0.0,
            MarginZone::Bottom => f.y > body_bottom && f.y <= page_height,
        })
        .collect();

    if items.is_empty() {
        return None;
    }

    items.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    let mut para = Paragraph::new();
    for item in &items {
        para.push_run(Run::new(&item.text));
    }

    // Detect alignment — use 15% of page center as threshold to avoid
    // false-positives on left-aligned text that starts near the center.
    let page_center = page_width / 2.0;
    if items.iter().all(|f| f.x > page_center) {
        para.alignment = Some(Alignment::Right);
    } else if items
        .iter()
        .all(|f| (f.x - page_center).abs() < page_center * 0.15)
    {
        para.alignment = Some(Alignment::Center);
    }

    Some(HeaderFooter {
        paragraphs: vec![para],
    })
}

// ---------------------------------------------------------------------------
// Page number detection
// ---------------------------------------------------------------------------

/// Check if the footer text on the first page looks like a page number.
///
/// Returns `Some(PageNumberFormat)` if the footer is just a page number
/// (e.g. "1", "i", "I", "a", "A"), otherwise `None`.
///
/// We check multiple pages to confirm — if different pages have different
/// consecutive numbers, it's definitely a page number rather than static text.
/// A single footer "i" (a word) or "5" (a static label) would be misclassified
/// without multi-page verification.
#[must_use]
pub fn detect_page_numbering(
    paged: &PagedDocument,
    margins: MarginsPt,
) -> Option<PageNumberFormat> {
    if paged.pages().is_empty() {
        return None;
    }

    // Extract footer text from the first page
    let first_footer = extract_footer_text_from_page(&paged.pages()[0].frame, margins)?;
    let first_trimmed = first_footer.trim();

    // Try to classify the text as a page number format
    let fmt = classify_page_number(first_trimmed)?;

    // If we have a second page, verify consecutiveness to avoid false positives
    if paged.pages().len() >= 2 {
        let second_footer = extract_footer_text_from_page(&paged.pages()[1].frame, margins);
        match second_footer {
            Some(ref text) => {
                let second_trimmed = text.trim();
                let fmt2 = classify_page_number(second_trimmed);
                // Both pages must have the same format
                if fmt2.as_ref() != Some(&fmt) {
                    return None;
                }
                // Values must be consecutive (page 2 value = page 1 value + 1)
                let val1 = page_number_value(first_trimmed, &fmt);
                let val2 = page_number_value(second_trimmed, &fmt);
                if val1 == 0 || val2 == 0 || val2 != val1 + 1 {
                    return None;
                }
            }
            // Second page has no footer text — can't confirm page numbering
            None => return None,
        }
    } else {
        // Single-page document: only accept a single-digit number "1" as
        // reasonably likely to be page numbering. Other formats ("i", "a",
        // etc.) are too ambiguous without a second page to confirm.
        if first_trimmed != "1" {
            return None;
        }
    }

    Some(fmt)
}

/// Get the numeric value of a page number string for a given format.
///
/// Returns 0 if the string cannot be parsed for the given format.
fn page_number_value(s: &str, fmt: &PageNumberFormat) -> u32 {
    match fmt {
        PageNumberFormat::Decimal => s.parse::<u32>().unwrap_or(0),
        PageNumberFormat::LowerRoman => roman_value(s, false),
        PageNumberFormat::UpperRoman => roman_value(s, true),
        PageNumberFormat::LowerLetter => {
            if s.len() == 1 {
                let c = s.chars().next().unwrap();
                if c.is_ascii_lowercase() {
                    return u32::from(c) - u32::from('a') + 1;
                }
            }
            0
        }
        PageNumberFormat::UpperLetter => {
            if s.len() == 1 {
                let c = s.chars().next().unwrap();
                if c.is_ascii_uppercase() {
                    return u32::from(c) - u32::from('A') + 1;
                }
            }
            0
        }
    }
}

/// Extract footer text from a single page frame (text in the bottom margin zone).
fn extract_footer_text_from_page(frame: &Frame, margins: MarginsPt) -> Option<String> {
    let page_width = frame.width().to_pt();
    let page_height = frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(frame, Point::zero(), &mut fragments);

    let (_body_top, body_bottom) = find_body_zone(
        page_width,
        page_height,
        Some(margins.top),
        Some(margins.bottom),
    );

    let footer_items: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| f.y > body_bottom && f.y <= page_height)
        .collect();

    if footer_items.is_empty() {
        return None;
    }

    let text: String = footer_items.iter().map(|f| f.text.as_str()).collect();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Classify a string as a page number format.
///
/// - "1", "2", ... → `Decimal`
/// - "i", "ii", "iii", "iv", ... → `LowerRoman`
/// - "I", "II", "III", "IV", ... → `UpperRoman`
/// - "a", "b", "c", ... → `LowerLetter`
/// - "A", "B", "C", ... → `UpperLetter`
fn classify_page_number(s: &str) -> Option<PageNumberFormat> {
    if s.is_empty() {
        return None;
    }

    // Decimal: pure digits
    if s.chars().all(|c| c.is_ascii_digit()) {
        return Some(PageNumberFormat::Decimal);
    }

    // Roman numerals (lowercase): i, ii, iii, iv, v, vi, vii, viii, ix, x, ...
    if is_lower_roman(s) {
        return Some(PageNumberFormat::LowerRoman);
    }

    // Roman numerals (uppercase): I, II, III, IV, V, ...
    if is_upper_roman(s) {
        return Some(PageNumberFormat::UpperRoman);
    }

    // Single letter: a-z or A-Z (page 1 = a, page 2 = b, etc.)
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        if c.is_ascii_lowercase() {
            return Some(PageNumberFormat::LowerLetter);
        }
        if c.is_ascii_uppercase() {
            return Some(PageNumberFormat::UpperLetter);
        }
    }

    None
}

/// Check if a string is a valid lowercase Roman numeral.
fn is_lower_roman(s: &str) -> bool {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'))
    {
        return false;
    }
    // Validate by converting and checking it's a reasonable page number
    roman_value(s, false) > 0
}

/// Check if a string is a valid uppercase Roman numeral.
fn is_upper_roman(s: &str) -> bool {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
    {
        return false;
    }
    roman_value(s, true) > 0
}

/// Compute the numeric value of a Roman numeral string.
fn roman_value(s: &str, uppercase: bool) -> u32 {
    let val = |c: char| -> u32 {
        match if uppercase { c.to_ascii_lowercase() } else { c } {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => 0,
        }
    };

    let chars: Vec<char> = s.chars().collect();
    let mut total: u32 = 0;
    for i in 0..chars.len() {
        let curr = val(chars[i]);
        let next = if i + 1 < chars.len() {
            val(chars[i + 1])
        } else {
            0
        };
        if curr < next {
            total = total.wrapping_sub(curr);
        } else {
            total = total.wrapping_add(curr);
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Unified per-run style recovery from PagedDocument
// ---------------------------------------------------------------------------

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
        collect_styles_from_frame(&page.frame, Point::zero(), page_width, &mut items);
    }
    items
}

fn collect_styles_from_frame(
    frame: &Frame,
    offset: Point,
    page_width: f64,
    items: &mut Vec<PagedRunStyle>,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if text.is_empty() {
                    continue;
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
                    is_bold: info.variant.weight.to_number() >= 700,
                    is_italic: matches!(
                        info.variant.style,
                        typst_library::text::FontStyle::Italic
                            | typst_library::text::FontStyle::Oblique
                    ),
                    is_math_font,
                    is_whitespace,
                    x: abs_x.to_pt(),
                    text_width: text_item.width().to_pt(),
                    page_width,
                });
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_styles_from_frame(&group.frame, new_offset, page_width, items);
            }
            _ => {}
        }
    }
}

/// Detect the most common font families (split by script) and size from
/// rendered text items.  Returns `(ascii_font, cjk_font, body_size_half_pt)`.
///
/// Splitting by script prevents a CJK-dominated document from picking a CJK
/// fallback font as the ASCII baseline (which would cause every Latin run to
/// get a spurious font override) and vice-versa.
fn detect_rendered_body_style(styles: &[PagedRunStyle]) -> (String, String, u32) {
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
    let body_font_ascii = ascii_font_counts
        .into_iter()
        .max_by_key(|(f, c)| (*c, std::cmp::Reverse(*f)))
        .map_or_else(|| "Times New Roman".to_string(), |(f, _)| f.to_string());
    let body_font_cjk = cjk_font_counts
        .into_iter()
        .max_by_key(|(f, c)| (*c, std::cmp::Reverse(*f)))
        .map_or_else(|| body_font_ascii.clone(), |(f, _)| f.to_string());
    let body_size = size_counts
        .into_iter()
        .max_by_key(|(s, c)| (*c, std::cmp::Reverse(*s)))
        .map_or(21, |(s, _)| s);

    (body_font_ascii, body_font_cjk, body_size)
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

/// Apply all per-run styles (color, font, size, bold, italic) and paragraph
/// alignment from the `PagedDocument` to the document model in a single pass.
pub fn apply_styles_from_paged(paged: &PagedDocument, doc: &mut typort_ooxml::document::Document) {
    let paged_styles = collect_paged_run_styles(paged);
    if paged_styles.is_empty() {
        return;
    }

    let (rendered_ascii, rendered_cjk, rendered_body_size_half_pt) =
        detect_rendered_body_style(&paged_styles);
    let body_size_half_pt = rendered_body_size_half_pt;

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

    let (span_overrides, text_overrides) = build_style_override_maps(
        &paged_styles,
        body_size_half_pt,
        &rendered_ascii,
        &rendered_cjk,
        declared_ascii,
        &body_cjk_fonts,
    );

    // Apply run-level overrides to body elements
    apply_overrides_to_elements(&mut doc.body.elements, &span_overrides, &text_overrides);

    // Apply run-level overrides to footnotes
    for footnote in &mut doc.footnotes {
        for inline in &mut footnote.content {
            if let InlineElement::Text(run) = inline {
                apply_override_to_run(run, &span_overrides, &text_overrides, None);
            }
        }
    }

    // Apply paragraph alignment from x-positions
    apply_paragraph_alignment(paged, &paged_styles, doc);

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

/// Windows language IDs (LCIDs) for the localized font-name records that match a
/// document's declared East-Asian language tag, in preference order. Returns an
/// empty list for non-CJK tags (no localized name to prefer over the typographic
/// English family). Drives [`localize_cjk_font_names`].
fn cjk_localized_lang_ids(lang_tag: &str) -> Vec<u16> {
    let tag = lang_tag.to_ascii_lowercase();
    let mut subtags = tag.split(['-', '_']);
    let primary = subtags.next().unwrap_or("");
    let rest: Vec<&str> = subtags.collect();
    match primary {
        "zh" => {
            // Traditional-script regions/script subtag prefer the Taiwan/HK/Macau
            // name records; everything else (CN, SG, Hans, or a bare `zh`) is
            // Simplified and prefers the PRC record. The English typographic
            // family stays the fallback if neither localized record exists.
            let traditional = rest
                .iter()
                .any(|p| matches!(*p, "tw" | "hk" | "mo" | "hant"));
            if traditional {
                vec![0x0404, 0x0C04, 0x1404, 0x0804]
            } else {
                vec![0x0804, 0x1004, 0x0404]
            }
        }
        "ja" => vec![0x0411],
        "ko" => vec![0x0412],
        _ => Vec::new(),
    }
}

/// Rewrite every East-Asian font name in `doc` to its localized form using
/// `name_map` (rendered English family → localized display name). Typst exposes
/// these CJK fonts only by their English typographic name (e.g. `SimSun`), so the
/// `w:eastAsia` strings that reach Word are English; Word/users expect the
/// localized name (`宋体`). Latin (`w:ascii`) fonts and families absent from the
/// map are left untouched. A no-op when `name_map` is empty.
fn localize_cjk_font_names(doc: &mut Document, name_map: &HashMap<String, String>) {
    if name_map.is_empty() {
        return;
    }
    if let Some(localized) = name_map.get(&doc.style.body_font_east_asia) {
        doc.style.body_font_east_asia = localized.clone();
    }
    doc.for_each_run_mut(&mut |run| localize_run_font(run, name_map));
}

/// Remap a single run's East-Asian font via `name_map`. Latin (`font_ascii`) is
/// never touched — only `w:eastAsia` carries the localizable CJK family.
fn localize_run_font(run: &mut Run, name_map: &HashMap<String, String>) {
    if let Some(family) = &run.font_east_asia
        && let Some(localized) = name_map.get(family)
    {
        run.font_east_asia = Some(localized.clone());
    }
}

/// The localized FAMILY (name ID 1) string from a font's name table for the first
/// matching Windows language id in `lang_ids`, or `None` if the face carries no
/// such localized record (then the English typographic family is kept).
fn localized_family_from_face(face: &ttf_parser::Face, lang_ids: &[u16]) -> Option<String> {
    for &want in lang_ids {
        for entry in face.names() {
            if entry.name_id == ttf_parser::name_id::FAMILY
                && entry.platform_id == ttf_parser::PlatformId::Windows
                && entry.language_id == want
                && let Some(name) = entry.to_string()
            {
                return Some(name);
            }
        }
    }
    None
}

/// Translate every East-Asian font name in `doc` to the localized display name
/// Word shows for the document's declared language (e.g. `SimSun` → `宋体`).
///
/// Typst indexes these CJK fonts only by their English typographic family
/// (`info.family`), so both the source-declared body default *and* the rendered
/// per-run families that reach `w:eastAsia` are English. The localized name lives
/// in each font's own `name` table; we read it from the **font book** (not the
/// rendered frames) so it works even when Typst substitutes a different face at
/// render time — e.g. Typst's known `SimSun`→fallback substitution (typst#6205),
/// where the body previews in Noto Serif SC yet the author clearly meant `SimSun`.
///
/// A no-op for non-CJK documents and for fonts that carry no differing localized
/// name (then the English family is kept). Pure-string translation only — it
/// never changes which glyphs Word renders, just the font *name* in the box.
pub fn localize_cjk_fonts(world: &dyn World, doc: &mut Document) {
    let lang_ids = cjk_localized_lang_ids(&doc.style.lang_east_asia);
    if lang_ids.is_empty() {
        return;
    }
    let mut name_map: HashMap<String, String> = HashMap::new();
    for family in collect_cjk_font_names(doc) {
        if let Some(localized) = localized_name_via_book(world, &family, &lang_ids)
            && localized != family
        {
            name_map.insert(family, localized);
        }
    }
    localize_cjk_font_names(doc, &name_map);
}

/// The distinct East-Asian font family names currently in `doc`: the body default
/// plus every per-run `font_east_asia` override (body, tables, bibliography,
/// hyperlinks, footnotes).
fn collect_cjk_font_names(doc: &Document) -> HashSet<String> {
    let mut names = HashSet::new();
    names.insert(doc.style.body_font_east_asia.clone());
    doc.for_each_run(&mut |run| {
        if let Some(f) = &run.font_east_asia {
            names.insert(f.clone());
        }
    });
    names
}

/// Look up `family` in the font book and read its localized FAMILY name for the
/// first matching `lang_ids` entry. Works for any installed font regardless of
/// whether it was selected during rendering.
fn localized_name_via_book(world: &dyn World, family: &str, lang_ids: &[u16]) -> Option<String> {
    // The book keys families by their lowercased English typographic name, and
    // Typst's suffix-trimming can group distinct files under one key (e.g.
    // `SimSun-ExtB` lands under `SimSun` — and it carries no localized record).
    // So scan every variant and take the first that advertises a localized name,
    // rather than the arbitrary first variant.
    //
    // But weight variants of one design can also share a key while carrying
    // DISTINCT English families: "Noto Serif SC Light" groups under "Noto Serif
    // SC", and its localized record would mislabel the requested family with the
    // weight name. So prefer a variant whose English typographic family (name ID
    // 1, LCID 0x0409) equals the requested family, and skip variants whose
    // English family differs from it.
    world
        .book()
        .select_family(&family.to_lowercase())
        .filter_map(|index| world.font(index))
        .find_map(|font| {
            // typst 0.15 removed `Font::ttf`; the `ttf_parser::Face` now lives on
            // `FontInstance`. Re-parse the face from the font's own buffer and
            // collection index (exactly what `Font::new` does internally) so we
            // can still read its name table for localized CJK family names.
            let face = ttf_parser::Face::parse(font.data(), font.index()).ok()?;
            // Skip variants whose English typographic family differs from the
            // requested one (e.g. a same-keyed "Noto Serif SC Light"). The
            // English family is the same FAMILY lookup with the en-US LCID.
            if localized_family_from_face(&face, &[0x0409])
                .is_some_and(|eng| !eng.eq_ignore_ascii_case(family))
            {
                return None;
            }
            localized_family_from_face(&face, lang_ids)
        })
}

/// Build per-span and per-text style override maps from paged run styles.
///
/// Compares each rendered run against the detected baselines (font + size)
/// and emits an override entry only when the run differs from the baseline.
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

        let has_override = color.is_some()
            || font_ascii.is_some()
            || font_east_asia.is_some()
            || size_override.is_some()
            || item.is_bold
            || item.is_italic;

        if !has_override {
            continue;
        }

        let ovr = RunStyleOverride {
            color,
            font_ascii,
            font_east_asia,
            size_half_pt: size_override,
            force_bold: if item.is_bold { Some(true) } else { None },
            force_italic: if item.is_italic { Some(true) } else { None },
        };

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
        if text_ovr.color.is_some()
            || text_ovr.font_ascii.is_some()
            || text_ovr.font_east_asia.is_some()
            || text_ovr.size_half_pt.is_some()
            || text_ovr.force_bold.is_some()
            || text_ovr.force_italic.is_some()
        {
            text_overrides
                .entry(item.text.clone())
                .or_default()
                .push(text_ovr);
        }
    }

    (span_overrides, text_overrides)
}

fn apply_override_to_run(
    run: &mut Run,
    span_overrides: &HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>>,
    text_overrides: &HashMap<String, Vec<RunStyleOverride>>,
    sibling_size_hint: Option<u32>,
) {
    let ovr = run
        .span
        .and_then(|s| span_overrides.get(&s))
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
            text_overrides.get(&run.text).and_then(|entries| {
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
    span_overrides: &HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>>,
    text_overrides: &HashMap<String, Vec<RunStyleOverride>>,
) {
    for element in elements.iter_mut() {
        typort_ooxml::document::for_each_paragraph_in_block_mut(element, &mut |p| {
            apply_overrides_to_paragraph(p, span_overrides, text_overrides);
        });
    }
}

fn apply_overrides_to_paragraph(
    para: &mut Paragraph,
    span_overrides: &HashMap<typst_syntax::Span, Vec<(String, RunStyleOverride)>>,
    text_overrides: &HashMap<String, Vec<RunStyleOverride>>,
) {
    // First pass: apply overrides to runs that have span-based matches or
    // unambiguous text matches.
    let mut has_ambiguous = false;
    for inline in &mut para.inlines {
        match inline {
            typort_ooxml::document::InlineElement::Text(run) => {
                let is_ambiguous = run.span.is_none()
                    && text_overrides.get(&run.text).is_some_and(|e| e.len() > 1);
                if is_ambiguous {
                    has_ambiguous = true;
                } else {
                    apply_override_to_run(run, span_overrides, text_overrides, None);
                }
            }
            typort_ooxml::document::InlineElement::Hyperlink { runs, .. } => {
                for run in runs {
                    apply_override_to_run(run, span_overrides, text_overrides, None);
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
                    && text_overrides.get(&run.text).is_some_and(|e| e.len() > 1);
                if is_ambiguous {
                    apply_override_to_run(run, span_overrides, text_overrides, sibling_size);
                }
            }
        }
    }
}

/// Apply paragraph alignment by cross-referencing rendered x-positions.
fn apply_paragraph_alignment(
    _paged: &PagedDocument,
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
        let tolerance = page_width * 0.05;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_localized_lang_ids_zh_cn_prefers_prc() {
        // zh-CN must prefer the PRC localized-name record (LCID 0x0804 → 宋体).
        assert_eq!(
            cjk_localized_lang_ids("zh-CN").first().copied(),
            Some(0x0804)
        );
    }

    #[test]
    fn cjk_localized_lang_ids_non_cjk_is_empty() {
        // A Latin document has no localized CJK name to prefer.
        assert!(cjk_localized_lang_ids("en-US").is_empty());
    }

    #[test]
    fn localize_cjk_font_names_remaps_body_default_and_runs() {
        // At this pipeline stage a zh-CN document carries English CJK family names
        // (SimSun/SimHei). The localized-name map translates them to what Word
        // shows in its font box; Latin fonts and unmapped families stay as-is.
        let mut doc = Document::new();
        doc.style.body_font_ascii = "Times New Roman".to_string();
        doc.style.body_font_east_asia = "SimSun".to_string();

        let mut para = Paragraph::new();
        let mut heading_run = Run::new("黑体标题");
        heading_run.font_east_asia = Some("SimHei".to_string());
        para.push_run(heading_run);
        let mut latin_run = Run::new("Latin");
        latin_run.font_ascii = Some("Arial".to_string());
        para.push_run(latin_run);
        let mut unmapped_run = Run::new("仿宋");
        unmapped_run.font_east_asia = Some("FangSong".to_string());
        para.push_run(unmapped_run);
        doc.body
            .elements
            .push(typort_ooxml::document::BlockElement::Paragraph(para));

        let name_map = HashMap::from([
            ("SimSun".to_string(), "宋体".to_string()),
            ("SimHei".to_string(), "黑体".to_string()),
        ]);
        localize_cjk_font_names(&mut doc, &name_map);

        assert_eq!(
            doc.style.body_font_east_asia, "宋体",
            "body default SimSun → 宋体"
        );
        let runs: Vec<&Run> = doc
            .body
            .elements
            .iter()
            .flat_map(|e| {
                if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
                    p.text_runs().collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            })
            .collect();
        assert_eq!(
            runs[0].font_east_asia.as_deref(),
            Some("黑体"),
            "per-run SimHei → 黑体"
        );
        assert_eq!(
            runs[1].font_ascii.as_deref(),
            Some("Arial"),
            "Latin ascii font is left untouched"
        );
        assert_eq!(
            runs[2].font_east_asia.as_deref(),
            Some("FangSong"),
            "a family absent from the map is left untouched"
        );
    }

    #[test]
    fn collect_hanging_indent_rules_records_set_and_reset() {
        // A non-zero rule then a reset (0em) rule are both recorded, in source
        // order, with the reset flagged `nonzero == false`.
        let src = typst_syntax::Source::detached(
            "Body.\n#set par(hanging-indent: 2em)\nRef one.\n\
             #set par(hanging-indent: 0em)\nBody again.\n",
        );
        let rules = collect_par_hanging_indent_rules(&src);
        assert_eq!(rules.len(), 2, "two par(hanging-indent) rules expected");
        assert!(rules[0].nonzero, "first rule (2em) is a non-zero indent");
        assert!(!rules[1].nonzero, "second rule (0em) is a reset");
        assert!(
            rules[0].offset < rules[1].offset,
            "rules are ordered by source position"
        );
    }

    #[test]
    fn collect_hanging_indent_rules_ignores_unrelated_par_args() {
        // first-line-indent is not a hanging indent; no rule is recorded.
        let src = typst_syntax::Source::detached("#set par(first-line-indent: 2em)\nText.\n");
        assert!(collect_par_hanging_indent_rules(&src).is_empty());
    }

    #[test]
    fn classify_decimal() {
        assert_eq!(classify_page_number("1"), Some(PageNumberFormat::Decimal));
        assert_eq!(classify_page_number("42"), Some(PageNumberFormat::Decimal));
        assert_eq!(classify_page_number("100"), Some(PageNumberFormat::Decimal));
    }

    #[test]
    fn classify_lower_roman() {
        assert_eq!(
            classify_page_number("i"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("ii"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("iv"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("xiv"),
            Some(PageNumberFormat::LowerRoman)
        );
    }

    #[test]
    fn classify_upper_roman() {
        assert_eq!(
            classify_page_number("I"),
            Some(PageNumberFormat::UpperRoman)
        );
        assert_eq!(
            classify_page_number("IV"),
            Some(PageNumberFormat::UpperRoman)
        );
        assert_eq!(
            classify_page_number("XII"),
            Some(PageNumberFormat::UpperRoman)
        );
    }

    #[test]
    fn classify_letters() {
        assert_eq!(
            classify_page_number("a"),
            Some(PageNumberFormat::LowerLetter)
        );
        assert_eq!(
            classify_page_number("z"),
            Some(PageNumberFormat::LowerLetter)
        );
        assert_eq!(
            classify_page_number("A"),
            Some(PageNumberFormat::UpperLetter)
        );
        assert_eq!(
            classify_page_number("Z"),
            Some(PageNumberFormat::UpperLetter)
        );
    }

    #[test]
    fn classify_non_page_numbers() {
        assert_eq!(classify_page_number(""), None);
        assert_eq!(classify_page_number("Draft"), None);
        assert_eq!(classify_page_number("Page 1"), None);
        assert_eq!(classify_page_number("hello"), None);
    }

    #[test]
    fn page_number_value_decimal() {
        assert_eq!(page_number_value("1", &PageNumberFormat::Decimal), 1);
        assert_eq!(page_number_value("5", &PageNumberFormat::Decimal), 5);
        assert_eq!(page_number_value("42", &PageNumberFormat::Decimal), 42);
    }

    #[test]
    fn page_number_value_roman() {
        assert_eq!(page_number_value("i", &PageNumberFormat::LowerRoman), 1);
        assert_eq!(page_number_value("ii", &PageNumberFormat::LowerRoman), 2);
        assert_eq!(page_number_value("iii", &PageNumberFormat::LowerRoman), 3);
        assert_eq!(page_number_value("iv", &PageNumberFormat::LowerRoman), 4);
        assert_eq!(page_number_value("v", &PageNumberFormat::LowerRoman), 5);
        assert_eq!(page_number_value("IX", &PageNumberFormat::UpperRoman), 9);
        assert_eq!(page_number_value("X", &PageNumberFormat::UpperRoman), 10);
    }

    #[test]
    fn page_number_value_letters() {
        assert_eq!(page_number_value("a", &PageNumberFormat::LowerLetter), 1);
        assert_eq!(page_number_value("b", &PageNumberFormat::LowerLetter), 2);
        assert_eq!(page_number_value("z", &PageNumberFormat::LowerLetter), 26);
        assert_eq!(page_number_value("A", &PageNumberFormat::UpperLetter), 1);
        assert_eq!(page_number_value("C", &PageNumberFormat::UpperLetter), 3);
    }

    #[test]
    fn consecutive_check_logic() {
        // Decimal: 1 -> 2 is consecutive
        let v1 = page_number_value("1", &PageNumberFormat::Decimal);
        let v2 = page_number_value("2", &PageNumberFormat::Decimal);
        assert_eq!(v2, v1 + 1);

        // Decimal: 5 -> 5 is NOT consecutive (static text)
        let v1 = page_number_value("5", &PageNumberFormat::Decimal);
        let v2 = page_number_value("5", &PageNumberFormat::Decimal);
        assert_ne!(v2, v1 + 1);

        // Roman: i -> ii is consecutive
        let v1 = page_number_value("i", &PageNumberFormat::LowerRoman);
        let v2 = page_number_value("ii", &PageNumberFormat::LowerRoman);
        assert_eq!(v2, v1 + 1);

        // Letter: a -> b is consecutive
        let v1 = page_number_value("a", &PageNumberFormat::LowerLetter);
        let v2 = page_number_value("b", &PageNumberFormat::LowerLetter);
        assert_eq!(v2, v1 + 1);
    }

    #[test]
    fn source_ast_extracts_text_size_and_par_spacing() {
        let source = r#"#set text(font: "Linux Libertine", size: 10.5pt)"#;
        let ovr = extract_source_style_overrides(source, &[]);
        assert_eq!(ovr.text_size_half_pt, Some(21));
        assert_eq!(
            ovr.text_font.as_deref(),
            Some(&["Linux Libertine".to_string()][..])
        );
    }

    #[test]
    fn source_ast_extracts_par_settings() {
        let source = r#"#set par(first-line-indent: 2em, spacing: 1.5em, justify: true)"#;
        let ovr = extract_source_style_overrides(source, &[]);
        assert_eq!(ovr.first_line_indent_em, Some(2.0));
        assert_eq!(ovr.par_spacing_em, Some(1.5));
        assert_eq!(ovr.justify, Some(true));
    }

    #[test]
    fn source_ast_extracts_page_margin() {
        let source = r#"#set page(margin: 2cm)"#;
        let ovr = extract_source_style_overrides(source, &[]);
        let expected = pt_to_twips(2.0_f64 * 72.0 / 2.54);
        assert_eq!(ovr.margin_top, Some(expected));
        assert_eq!(ovr.margin_left, Some(expected));
    }

    #[test]
    fn source_ast_complex_paper_pattern() {
        let source = r#"#set par(leading: 1.5em, first-line-indent: 2em)"#;
        let ovr = extract_source_style_overrides(source, &[]);
        assert!(
            ovr.first_line_indent_em.is_some(),
            "first-line-indent should be detected, got None"
        );
        assert!(
            ovr.first_line_indent_em.unwrap() > 0.0,
            "first-line-indent should be > 0"
        );
        assert!(ovr.par_leading_em.is_some(), "leading should be detected");
    }

    #[test]
    fn source_ast_scope_aware_show_template_body_size() {
        // The real global body size lives inside the show-template closure;
        // a 9pt helper closure and a 9pt nested block must NOT win.
        let body = r#"
#let helper() = {
  set text(size: 9pt)
  [helper text]
}
#let tmpl(body) = {
  set text(size: 12pt)
  body
}
= Heading
Body paragraph.
#block[
  #set text(size: 9pt)
  A small block.
]
"#;

        let with_call = format!("{body}\n#show: tmpl.with()");
        let names = extract_show_template_names_from_source(&with_call);
        assert_eq!(names, vec!["tmpl".to_string()]);
        let ovr = extract_source_style_overrides(&with_call, &names);
        assert_eq!(
            ovr.text_size_half_pt,
            Some(24),
            "show-template body size (12pt) must win over helper/block 9pt"
        );

        // Bare `#show: tmpl` form resolves identically.
        let bare = format!("{body}\n#show: tmpl");
        let bare_names = extract_show_template_names_from_source(&bare);
        assert_eq!(bare_names, vec!["tmpl".to_string()]);
        let ovr_bare = extract_source_style_overrides(&bare, &bare_names);
        assert_eq!(ovr_bare.text_size_half_pt, Some(24));

        // No template at all: a bare top-level `#set text` still resolves.
        let top_level = r#"#set text(size: 10.5pt)
= Heading
Body."#;
        let top_names = extract_show_template_names_from_source(top_level);
        assert!(top_names.is_empty());
        let ovr_top = extract_source_style_overrides(top_level, &top_names);
        assert_eq!(ovr_top.text_size_half_pt, Some(21));
    }
}
