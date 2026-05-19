//! `PagedDocument` -> page settings, document style, font detection.
//!
//! Extracts the most-common font family / size from rendered frames
//! and computes page dimensions + margins from the first page.
//! Also detects section breaks, headers/footers, and column layouts.

use std::collections::HashMap;

use typort_ooxml::document::{
    Alignment, DocumentStyle, FootnoteFormat, HeaderFooter, PageNumberFormat, PageSettings,
    Paragraph, ParagraphStyle, Run, SectionBreak, SectionBreakType,
};
use typst::layout::{Frame, FrameItem, PagedDocument, Point};

/// Extract document style (fonts, sizes, spacing) from the rendered `PagedDocument`.
///
/// Walks the first few pages' frames to find the most common font family and size,
/// which represent the body text styling.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
    let mut ascii_font_counts: HashMap<String, usize> = HashMap::new();
    let mut cjk_font_counts: HashMap<String, usize> = HashMap::new();
    let mut size_counts: HashMap<u32, usize> = HashMap::new();
    let mut y_positions: Vec<f64> = Vec::new();

    for page in paged.pages.iter().take(3) {
        collect_font_info_split(
            &page.frame,
            Point::zero(),
            &mut ascii_font_counts,
            &mut cjk_font_counts,
            &mut size_counts,
            &mut y_positions,
        );
    }

    // Detect body font (most common per script)
    let body_font_ascii = ascii_font_counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map_or_else(|| "Times New Roman".to_string(), |(f, _)| f.clone());

    let body_font_east_asia = cjk_font_counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map_or_else(|| body_font_ascii.clone(), |(f, _)| f.clone());

    // Detect body size (most common)
    let body_size_half_pt = size_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map_or(21, |(size, _)| *size);

    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = detect_first_line_indent(paged, body_pt);
    let line_spacing = detect_line_spacing(&mut y_positions, body_size_half_pt);

    // Detect code font (monospace font that isn't the body font)
    let code_font = ascii_font_counts
        .iter()
        .filter(|(f, _)| {
            let fl = f.to_lowercase();
            (fl.contains("mono") || fl.contains("courier") || fl.contains("consol")
                || fl.contains("fira code") || fl.contains("source code"))
                && f.as_str() != body_font_ascii
        })
        .max_by_key(|(_, c)| *c)
        .map_or_else(|| "Courier New".to_string(), |(f, _)| f.clone());

    // Detect sizes for code, footnotes, headings from actual rendered data
    let code_size_half_pt = detect_code_size(&size_counts, body_size_half_pt);
    let footnote_size_half_pt = detect_footnote_size(&size_counts, body_size_half_pt);
    let heading_sizes = detect_heading_sizes(&size_counts, body_size_half_pt);

    // Detect heading before/after spacing from y-position gaps around large text
    let (heading_spacing_before, heading_spacing_after) =
        detect_heading_spacing(&y_positions, &size_counts, body_size_half_pt, paged);

    DocumentStyle {
        body_font_ascii,
        body_font_east_asia,
        body_size_half_pt,
        line_spacing,
        first_line_indent_twips,
        footnote_format: FootnoteFormat::default(),
        code_font,
        heading_spacing_before,
        heading_spacing_after,
        code_size_half_pt,
        footnote_size_half_pt,
        heading_sizes,
        body_alignment: detect_justification(paged),
    }
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' |
        '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}' |
        '\u{AC00}'..='\u{D7AF}' | '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}'
    )
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn detect_first_line_indent(paged: &PagedDocument, body_pt: f64) -> u32 {
    let Some(page) = paged.pages.first() else {
        return (body_pt * 20.0 * 2.0).round() as u32;
    };

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, Point::zero(), &mut fragments);

    if fragments.len() < 4 {
        return (body_pt * 20.0 * 2.0).round() as u32;
    }

    // Group by y (lines), find the left-most x per line
    let page_width = page.frame.width().to_pt();
    let (body_top, body_bottom) = find_body_zone(page_width, page.frame.height().to_pt());

    let body_frags: Vec<&TextFragment> = fragments.iter()
        .filter(|f| f.y >= body_top && f.y <= body_bottom)
        .collect();

    let mut left_edges: Vec<f64> = body_frags.iter().map(|f| f.x).collect();
    left_edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if left_edges.len() < 2 {
        return (body_pt * 20.0 * 2.0).round() as u32;
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
            return (indent_pt * 20.0).round() as u32;
        }
    }

    // Fallback: 2 chars wide
    (body_pt * 20.0 * 2.0).round() as u32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn detect_line_spacing(y_positions: &mut Vec<f64>, body_size_half_pt: u32) -> u32 {
    if y_positions.len() < 2 {
        return 360;
    }
    y_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    y_positions.dedup_by(|a, b| (*a - *b).abs() < 0.5);

    let body_pt = f64::from(body_size_half_pt) / 2.0;
    // Collect gaps between consecutive lines that are plausible line spacing
    // (between 0.8x and 3x the font size)
    let mut gaps: Vec<f64> = Vec::new();
    for pair in y_positions.windows(2) {
        let gap = pair[1] - pair[0];
        if gap > body_pt * 0.8 && gap < body_pt * 3.0 {
            gaps.push(gap);
        }
    }
    if gaps.is_empty() {
        return 360;
    }
    // Use the median gap as the representative line pitch
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_pitch = gaps[gaps.len() / 2];
    // Convert: Word line spacing = (pitch / font_size) * 240
    let ratio = median_pitch / body_pt;
    let spacing = (ratio * 240.0).round() as u32;
    // Clamp to reasonable range
    spacing.clamp(200, 600)
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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn detect_justification(paged: &PagedDocument) -> String {
    // Collect right-edge x-positions of body text lines from the first few pages.
    // We group text items by y-position (line), then compute the right edge per line.
    let mut line_items: Vec<(f64, f64)> = Vec::new(); // (y, right_edge_x)

    for page in paged.pages.iter().take(3) {
        let page_width = page.frame.width().to_pt();
        let page_height = page.frame.height().to_pt();
        let (body_top, body_bottom) = find_body_zone(page_width, page_height);
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
                collect_right_edges(
                    &group.frame,
                    new_offset,
                    body_top,
                    body_bottom,
                    items,
                );
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
        .max_by_key(|(_, c)| *c)
        .map_or(body_size.saturating_sub(3).max(14), |(sz, _)| *sz)
}

/// Detect footnote text size: the smallest size with significant usage.
fn detect_footnote_size(size_counts: &HashMap<u32, usize>, body_size: u32) -> u32 {
    size_counts
        .iter()
        .filter(|(sz, count)| **sz < body_size && **count >= 3 && **sz >= 12)
        .min_by_key(|(sz, _)| *sz)
        .map_or(body_size.saturating_sub(3).max(14), |(sz, _)| *sz)
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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn detect_heading_spacing(
    _y_positions: &[f64],
    _size_counts: &HashMap<u32, usize>,
    body_size: u32,
    paged: &PagedDocument,
) -> (u32, u32) {
    let heading_min_size = body_size + 1;
    // Collect (y, size_half_pt) pairs from the first page
    let Some(page) = paged.pages.first() else {
        return (240, 120);
    };
    let mut items: Vec<(f64, u32)> = Vec::new();
    collect_y_and_size(&page.frame, Point::zero(), &mut items);
    if items.len() < 3 {
        return (240, 120);
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    items.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);

    let body_pt = f64::from(body_size) / 2.0;
    let normal_line_pitch = body_pt * 1.6; // approximate

    let mut before_gaps = Vec::new();
    let mut after_gaps = Vec::new();

    for (i, &(y, sz)) in items.iter().enumerate() {
        if sz >= heading_min_size {
            // Gap before heading: distance from previous item
            if i > 0 {
                let gap = y - items[i - 1].0;
                if gap > normal_line_pitch && gap < body_pt * 10.0 {
                    before_gaps.push(gap);
                }
            }
            // Gap after heading: distance to next item
            if i + 1 < items.len() {
                let gap = items[i + 1].0 - y;
                if gap > 0.0 && gap < body_pt * 10.0 {
                    after_gaps.push(gap);
                }
            }
        }
    }

    let before_twips = if before_gaps.is_empty() {
        240
    } else {
        before_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = before_gaps[before_gaps.len() / 2];
        // Subtract one normal line pitch to get the extra spacing
        let extra = (median - normal_line_pitch).max(0.0);
        (extra * 20.0).round() as u32
    };
    let after_twips = if after_gaps.is_empty() {
        120
    } else {
        after_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = after_gaps[after_gaps.len() / 2];
        let extra = (median - normal_line_pitch).max(0.0);
        (extra * 20.0).round() as u32
    };
    (before_twips.clamp(0, 1000), after_twips.clamp(0, 500))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_y_and_size(frame: &Frame, offset: Point, items: &mut Vec<(f64, u32)>) {
    for (pos, item) in frame.items() {
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let size_half_pt = (text_item.size.to_pt() * 2.0).round() as u32;
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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_font_info_split(
    frame: &Frame,
    offset: Point,
    ascii_fonts: &mut HashMap<String, usize>,
    cjk_fonts: &mut HashMap<String, usize>,
    size_counts: &mut HashMap<u32, usize>,
    y_positions: &mut Vec<f64>,
) {
    for (pos, item) in frame.items() {
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let family = text_item.font.info().family.clone();
                let size_half_pt = (text_item.size.to_pt() * 2.0).round() as u32;
                let glyph_count = text_item.glyphs.len();
                *size_counts.entry(size_half_pt).or_insert(0) += glyph_count;
                y_positions.push(abs_y.to_pt());

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
                    &group.frame, new_offset, ascii_fonts, cjk_fonts, size_counts, y_positions,
                );
            }
            _ => {}
        }
    }
}

/// Extract page dimensions from the `PagedDocument` and apply to `PageSettings`.
pub fn extract_page_settings(paged: &PagedDocument, settings: &mut PageSettings) {
    let Some(page) = paged.pages.first() else {
        return;
    };
    let m = extract_page_metrics(&page.frame);
    settings.width_twips = m.width_twips;
    settings.height_twips = m.height_twips;
    settings.margin_top = m.margin_top;
    settings.margin_bottom = m.margin_bottom;
    settings.margin_left = m.margin_left;
    settings.margin_right = m.margin_right;
}

/// Recursively collect content bounding box from frame items.
fn collect_content_bounds(
    frame: &Frame,
    offset: Point,
    min_x: &mut f64,
    max_x: &mut f64,
    min_y: &mut f64,
    max_y: &mut f64,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let x = abs_x.to_pt();
                let y = abs_y.to_pt();
                let w = text_item.width().to_pt();
                if x < *min_x {
                    *min_x = x;
                }
                if x + w > *max_x {
                    *max_x = x + w;
                }
                if y < *min_y {
                    *min_y = y;
                }
                let h = text_item.size.to_pt();
                if y + h > *max_y {
                    *max_y = y + h;
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_content_bounds(&group.frame, new_offset, min_x, max_x, min_y, max_y);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Section break detection
// ---------------------------------------------------------------------------

/// Settings extracted from a single page for comparison purposes.
#[derive(Debug, Clone)]
struct PageMetrics {
    width_twips: u32,
    height_twips: u32,
    margin_top: u32,
    margin_bottom: u32,
    margin_left: u32,
    margin_right: u32,
}

/// Extract page metrics from a single page frame.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_page_metrics(frame: &Frame) -> PageMetrics {
    let page_width = frame.width().to_pt();
    let page_height = frame.height().to_pt();

    let mut min_x = page_width;
    let mut max_x: f64 = 0.0;
    let mut min_y = page_height;
    let mut max_y: f64 = 0.0;

    collect_content_bounds(frame, Point::zero(), &mut min_x, &mut max_x, &mut min_y, &mut max_y);

    let (margin_left, margin_right, margin_top, margin_bottom) = if min_x < max_x && min_y < max_y
    {
        let ml = (min_x * 20.0).round().max(0.0) as u32;
        let mr = ((page_width - max_x) * 20.0).round().max(0.0) as u32;
        let mt = (min_y * 20.0).round().max(0.0) as u32;
        let mb = ((page_height - max_y) * 20.0).round().max(0.0) as u32;
        (
            if ml >= 100 { ml } else { 1440 },
            if mr >= 100 { mr } else { 1440 },
            if mt >= 100 { mt } else { 1440 },
            if mb >= 100 { mb } else { 1440 },
        )
    } else {
        (1440, 1440, 1440, 1440)
    };

    PageMetrics {
        width_twips: (page_width * 20.0).round() as u32,
        height_twips: (page_height * 20.0).round() as u32,
        margin_top,
        margin_bottom,
        margin_left,
        margin_right,
    }
}

/// Check if two page metrics differ enough to warrant a section break.
fn metrics_differ(a: &PageMetrics, b: &PageMetrics) -> bool {
    // Use a tolerance of 20 twips (1pt) to avoid false positives from rounding
    let tol = 20;
    a.width_twips.abs_diff(b.width_twips) > tol
        || a.height_twips.abs_diff(b.height_twips) > tol
        || a.margin_top.abs_diff(b.margin_top) > tol
        || a.margin_bottom.abs_diff(b.margin_bottom) > tol
        || a.margin_left.abs_diff(b.margin_left) > tol
        || a.margin_right.abs_diff(b.margin_right) > tol
}

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
pub fn detect_section_breaks(paged: &PagedDocument) -> Vec<DetectedSection> {
    if paged.pages.len() < 2 {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut prev_metrics = extract_page_metrics(&paged.pages[0].frame);

    for i in 1..paged.pages.len() {
        let curr_metrics = extract_page_metrics(&paged.pages[i].frame);
        if metrics_differ(&prev_metrics, &curr_metrics) {
            // Section break between page i-1 and page i.
            // The section that ends gets the prev_metrics as its settings.
            sections.push(DetectedSection {
                start_page: i,
                page_settings: PageSettings {
                    width_twips: prev_metrics.width_twips,
                    height_twips: prev_metrics.height_twips,
                    margin_top: prev_metrics.margin_top,
                    margin_bottom: prev_metrics.margin_bottom,
                    margin_left: prev_metrics.margin_left,
                    margin_right: prev_metrics.margin_right,
                    columns: None,
                    column_spacing: None,
                },
            });
        }
        prev_metrics = curr_metrics;
    }

    sections
}

/// Apply section breaks to the document model.
///
/// Given detected section boundaries from `PagedDocument` and the total
/// body element count, place `SectionBreak` on the appropriate paragraphs.
/// This uses a proportional mapping: if the document has N pages and M body
/// elements, the break after page P is placed at approximately element
/// `P * M / N`.
pub fn apply_section_breaks(
    doc: &mut typort_ooxml::document::Document,
    sections: &[DetectedSection],
    total_pages: usize,
) {
    if sections.is_empty() || total_pages == 0 {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    for section in sections {
        // Proportional mapping: break before page `start_page` means the
        // section break goes on the last element before that page's content.
        let approx_idx = section.start_page * total_elements / total_pages;
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
/// Typst renders headers in the top margin area and footers in the
/// bottom margin area. We use the computed default margin as a boundary
/// to separate these zones.
///
/// Returns `(body_top, body_bottom)` in pt — the y-range of the body zone.
fn find_body_zone(page_width: f64, page_height: f64) -> (f64, f64) {
    let margin = default_margin_pt(page_width, page_height);
    // Body starts at the margin line and ends at the bottom margin line.
    // Headers are positioned at margin * (1 - header_ascent) where header_ascent
    // defaults to 0.3, so header text is at ~margin * 0.7 from top.
    // Use margin * 0.9 as the boundary to safely include all header content.
    let body_top = margin * 0.9;
    let body_bottom = page_height - margin * 0.9;
    (body_top, body_bottom)
}

/// Extract header content from the top margin area of the first page.
pub fn extract_header(paged: &PagedDocument) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Top)
}

/// Extract footer content from the bottom margin area of the first page.
pub fn extract_footer(paged: &PagedDocument) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Bottom)
}

enum MarginZone {
    Top,
    Bottom,
}

#[allow(clippy::needless_pass_by_value)]
fn extract_margin_zone(paged: &PagedDocument, zone: MarginZone) -> Option<HeaderFooter> {
    let page = paged.pages.first()?;
    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, Point::zero(), &mut fragments);

    let (body_top, body_bottom) = find_body_zone(page_width, page_height);

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
pub fn detect_page_numbering(paged: &PagedDocument) -> Option<PageNumberFormat> {
    if paged.pages.is_empty() {
        return None;
    }

    // Extract footer text from the first page
    let first_footer = extract_footer_text_from_page(&paged.pages[0].frame)?;
    let first_trimmed = first_footer.trim();

    // Try to classify the text as a page number format
    let fmt = classify_page_number(first_trimmed)?;

    // If we have a second page, verify consecutiveness to avoid false positives
    if paged.pages.len() >= 2 {
        let second_footer = extract_footer_text_from_page(&paged.pages[1].frame);
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
fn extract_footer_text_from_page(frame: &Frame) -> Option<String> {
    let page_width = frame.width().to_pt();
    let page_height = frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(frame, Point::zero(), &mut fragments);

    let (_body_top, body_bottom) = find_body_zone(page_width, page_height);

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
    if s.is_empty() || !s.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'))
    {
        return false;
    }
    // Validate by converting and checking it's a reasonable page number
    roman_value(s, false) > 0
}

/// Check if a string is a valid uppercase Roman numeral.
fn is_upper_roman(s: &str) -> bool {
    if s.is_empty() || !s.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
    {
        return false;
    }
    roman_value(s, true) > 0
}

/// Compute the numeric value of a Roman numeral string.
fn roman_value(s: &str, uppercase: bool) -> u32 {
    let val = |c: char| -> u32 {
        match if uppercase {
            c.to_ascii_lowercase()
        } else {
            c
        } {
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
// Column detection
// ---------------------------------------------------------------------------

/// Detect the number of columns from the text layout of the first page.
///
/// Analyzes left-edge x-positions of text items to find distinct column groups.
/// In a multi-column layout, text items cluster at distinct x positions — one
/// per column's left margin. We detect columns by grouping these left edges.
/// Returns `None` if single-column (the default), or `Some(n)` for n columns.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::missing_panics_doc
)]
pub fn detect_columns(paged: &PagedDocument) -> Option<u32> {
    let page = paged.pages.first()?;
    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    // Collect all text fragments with positions
    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, Point::zero(), &mut fragments);

    // Filter to body-area text only (exclude headers/footers using margin-based zones)
    let (body_top, body_bottom) = find_body_zone(page_width, page_height);
    let body_frags: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| f.y >= body_top && f.y <= body_bottom)
        .collect();

    if body_frags.len() < 4 {
        return None;
    }

    // Collect the left-edge x-positions of all body text fragments.
    // In a multi-column layout, most text items start at one of N distinct
    // x-positions (one per column's left margin). Some items (headings, indented
    // text) may start at different positions, but the dominant clusters reveal columns.
    let mut x_starts: Vec<f64> = body_frags.iter().map(|f| f.x).collect();
    x_starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Group x-positions into clusters using a tolerance of 5pt.
    // Each cluster represents a potential column left margin.
    let cluster_tol = 5.0;
    let mut clusters: Vec<(f64, usize)> = Vec::new(); // (center_x, count)
    for &x in &x_starts {
        let found = clusters.iter_mut().find(|(cx, _)| (x - *cx).abs() < cluster_tol);
        if let Some((cx, count)) = found {
            // Update running average
            *cx = (*cx * (*count as f64) + x) / (*count as f64 + 1.0);
            *count += 1;
        } else {
            clusters.push((x, 1));
        }
    }

    // Sort clusters by x-position
    clusters.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Filter to significant clusters (at least 3 text items)
    let significant: Vec<(f64, usize)> = clusters
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .collect();

    if significant.len() < 2 {
        return None;
    }

    // Check if the significant clusters are separated by a substantial gap.
    // In a 2-column layout, the gap between the first cluster (left column)
    // and the second cluster (right column) should be at least 20% of page width.
    // For 3 columns, we'd see 3 evenly spaced clusters.
    let min_gap = page_width * 0.15; // at least 15% of page width

    // Find clusters that are well-separated
    let mut column_clusters: Vec<f64> = vec![significant[0].0];
    for &(cx, _) in &significant[1..] {
        let last = *column_clusters.last().unwrap();
        if cx - last >= min_gap {
            column_clusters.push(cx);
        }
    }

    let n_cols = column_clusters.len();
    if (2..=4).contains(&n_cols) {
        // Verify: each column cluster should be roughly evenly spaced
        // and columns should span the page width reasonably
        let first_col = column_clusters[0];
        let last_col = *column_clusters.last().unwrap();
        let span = last_col - first_col;
        if span > page_width * 0.3 {
            return Some(n_cols as u32);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Heading alignment detection from PagedDocument
// ---------------------------------------------------------------------------

/// A text item from the paged output with position and width info.
struct PagedTextItem {
    x: f64,
    text: String,
    text_width: f64,
    page_width: f64,
}

/// Collect text items from the body zone of all pages with their x-positions.
fn collect_body_text_items(paged: &PagedDocument) -> Vec<PagedTextItem> {
    let mut items = Vec::new();
    for page in &paged.pages {
        let page_width = page.frame.width().to_pt();
        let page_height = page.frame.height().to_pt();
        let (body_top, body_bottom) = find_body_zone(page_width, page_height);
        collect_body_text_items_from_frame(
            &page.frame,
            Point::zero(),
            page_width,
            body_top,
            body_bottom,
            &mut items,
        );
    }
    items
}

fn collect_body_text_items_from_frame(
    frame: &Frame,
    offset: Point,
    page_width: f64,
    body_top: f64,
    body_bottom: f64,
    items: &mut Vec<PagedTextItem>,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let y = abs_y.to_pt();
                // Only include body-zone text
                if y >= body_top && y <= body_bottom {
                    let text = text_item.text.to_string();
                    if !text.is_empty() {
                        items.push(PagedTextItem {
                            x: abs_x.to_pt(),
                            text,
                            text_width: text_item.width().to_pt(),
                            page_width,
                        });
                    }
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_body_text_items_from_frame(
                    &group.frame,
                    new_offset,
                    page_width,
                    body_top,
                    body_bottom,
                    items,
                );
            }
            _ => {}
        }
    }
}

/// Detect alignment of heading paragraphs by cross-referencing with the
/// `PagedDocument`'s rendered text positions.
///
/// For each heading in the document model, finds its text in the paged output
/// and determines whether it is centered, right-aligned, or left-aligned based
/// on the x-position relative to the page width.
pub fn apply_heading_alignment_from_paged(
    paged: &PagedDocument,
    doc: &mut typort_ooxml::document::Document,
) {
    let paged_items = collect_body_text_items(paged);
    if paged_items.is_empty() {
        return;
    }

    for element in &mut doc.body.elements {
        let typort_ooxml::document::BlockElement::Paragraph(p) = element else {
            continue;
        };
        if !matches!(p.style, Some(ParagraphStyle::Heading(_))) {
            continue;
        }
        // Skip if alignment is already set explicitly
        if p.alignment.is_some() {
            continue;
        }

        // Collect heading text from runs
        let heading_text = p.text_content();
        if heading_text.is_empty() {
            continue;
        }

        // Find the first run's text in the paged items
        let first_run_text = p.text_runs().next().map_or("", |r| r.text.as_str());
        if first_run_text.is_empty() {
            continue;
        }

        // Find all paged items that match the heading text.
        // We look for the first run text to identify the heading's rendered position.
        let matching: Vec<&PagedTextItem> = paged_items
            .iter()
            .filter(|item| item.text.contains(first_run_text) || first_run_text.contains(&item.text))
            .collect();

        if matching.is_empty() {
            continue;
        }

        // Use the first match to determine alignment.
        // For a heading that might span multiple text items, find all items
        // whose text is a substring of the heading text.
        let heading_items: Vec<&PagedTextItem> = paged_items
            .iter()
            .filter(|item| heading_text.contains(&item.text))
            .collect();

        if heading_items.is_empty() {
            continue;
        }

        // Compute the bounding box of the heading text
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

        // Determine alignment based on text position relative to page
        // Use a tolerance of 5% of page width
        let tolerance = page_width * 0.05;

        if (text_center - page_center).abs() < tolerance {
            p.alignment = Some(Alignment::Center);
        } else if min_x > page_center {
            p.alignment = Some(Alignment::Right);
        }
        // Left alignment is the default, no need to set it
    }
}

// ---------------------------------------------------------------------------
// Text color detection from PagedDocument
// ---------------------------------------------------------------------------

/// A text item from the paged output with color info.
struct PagedColorItem {
    text: String,
    /// Hex color string (6 uppercase hex digits, e.g. "FF0000"), or None if black.
    color_hex: Option<String>,
    /// Source spans from glyphs — used for precise matching.
    spans: Vec<typst_syntax::Span>,
}

/// Collect text items with their fill color from the paged output.
fn collect_text_colors(paged: &PagedDocument) -> Vec<PagedColorItem> {
    let mut items = Vec::new();
    for page in &paged.pages {
        collect_text_colors_from_frame(&page.frame, &mut items);
    }
    items
}

fn collect_text_colors_from_frame(frame: &Frame, items: &mut Vec<PagedColorItem>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if text.is_empty() {
                    continue;
                }
                let color_hex = extract_non_black_color(&text_item.fill);
                let spans: Vec<typst_syntax::Span> = text_item.glyphs.iter()
                    .map(|g| g.span.0)
                    .collect();
                items.push(PagedColorItem { text, color_hex, spans });
            }
            FrameItem::Group(group) => {
                collect_text_colors_from_frame(&group.frame, items);
            }
            _ => {}
        }
    }
}

/// Extract a hex color string from a Paint if it is not black.
///
/// Returns `Some("FF0000")` for red, `None` for black or near-black.
fn extract_non_black_color(paint: &typst_library::visualize::Paint) -> Option<String> {
    let typst_library::visualize::Paint::Solid(color) = paint else {
        return None;
    };
    let hex = color.to_hex();
    let hex_str = hex.as_str();
    // to_hex() returns e.g. "#ff0000" or "#ff000080" (with alpha)
    // Strip the '#' prefix
    let hex_digits = hex_str.strip_prefix('#').unwrap_or(hex_str);
    // Check if it's black (000000) — skip those
    if hex_digits.starts_with("000000") {
        return None;
    }
    // Return the first 6 hex digits (RGB) in uppercase for Word compatibility
    let rgb = &hex_digits[..6.min(hex_digits.len())];
    Some(rgb.to_uppercase())
}

/// Apply text colors from the `PagedDocument` to runs in the document model.
///
/// For each run in the document, finds matching text in the paged output and
/// applies the detected color if it is not black.
pub fn apply_text_colors_from_paged(
    paged: &PagedDocument,
    doc: &mut typort_ooxml::document::Document,
) {
    let paged_colors = collect_text_colors(paged);
    if paged_colors.is_empty() {
        return;
    }

    // Build a Span → color lookup for precise matching
    let mut span_colors: HashMap<typst_syntax::Span, String> = HashMap::new();
    // Also keep text-based fallback for runs without spans
    let mut text_colors: HashMap<String, String> = HashMap::new();

    for item in &paged_colors {
        if let Some(color) = &item.color_hex {
            for &span in &item.spans {
                if !span.is_detached() {
                    span_colors.insert(span, color.clone());
                }
            }
            text_colors.insert(item.text.clone(), color.clone());
        }
    }

    if span_colors.is_empty() && text_colors.is_empty() {
        return;
    }

    apply_colors_to_elements(&mut doc.body.elements, &span_colors, &text_colors);

    for footnote in &mut doc.footnotes {
        for run in &mut footnote.content {
            let matched = run.span
                .and_then(|s| span_colors.get(&s))
                .or_else(|| text_colors.get(run.text.as_str()));
            if let Some(color) = matched {
                run.color = Some(color.clone());
            }
        }
    }
}

fn apply_colors_to_elements(
    elements: &mut [typort_ooxml::document::BlockElement],
    span_colors: &HashMap<typst_syntax::Span, String>,
    text_colors: &HashMap<String, String>,
) {
    for element in elements.iter_mut() {
        match element {
            typort_ooxml::document::BlockElement::Paragraph(p) => {
                apply_colors_to_paragraph(p, span_colors, text_colors);
            }
            typort_ooxml::document::BlockElement::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        for para in &mut cell.paragraphs {
                            apply_colors_to_paragraph(para, span_colors, text_colors);
                        }
                    }
                }
            }
        }
    }
}

fn apply_color_to_run(
    run: &mut Run,
    span_colors: &HashMap<typst_syntax::Span, String>,
    text_colors: &HashMap<String, String>,
) {
    let matched = run.span
        .and_then(|s| span_colors.get(&s))
        .or_else(|| text_colors.get(&run.text));
    if let Some(color) = matched {
        run.color = Some(color.clone());
    }
}

fn apply_colors_to_paragraph(
    para: &mut Paragraph,
    span_colors: &HashMap<typst_syntax::Span, String>,
    text_colors: &HashMap<String, String>,
) {
    for inline in &mut para.inlines {
        match inline {
            typort_ooxml::document::InlineElement::Text(run) => {
                apply_color_to_run(run, span_colors, text_colors);
            }
            typort_ooxml::document::InlineElement::Hyperlink { runs, .. } => {
                for run in runs {
                    apply_color_to_run(run, span_colors, text_colors);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
