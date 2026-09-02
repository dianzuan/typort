//! Rendered-line extraction from paged frames.

use std::collections::{BTreeMap, HashMap};

use typort_ooxml::document::{BlockElement, Document, ParagraphStyle, Run};
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

/// Minimum substantive emitted-heading run length used for title detection.
const MIN_HEADING_RUN_CHARS: usize = 2;
/// Default body size in points when the recovery detector has no sample.
const DEFAULT_RECOVERY_BODY_SIZE_PT: f64 = 10.5;
/// Vertical bucket height in points used to assemble rendered lines.
const LINE_Y_BUCKET_PT: f64 = 8.0;
/// Default page width in points when paged geometry has no first page.
const DEFAULT_PAGE_WIDTH_PT: f64 = 595.0;
/// Page-width fraction contributing to the x-cluster gap threshold.
const CLUSTER_PAGE_WIDTH_RATIO: f64 = 0.06;
/// Absolute minimum x-cluster gap in points.
const MIN_CLUSTER_GAP_PT: f64 = 20.0;
/// Font-size multiple contributing to the x-cluster gap threshold.
const CLUSTER_FONT_SIZE_MULTIPLE: f64 = 5.0;
/// Minimum x-cluster count needed to recognize multi-column content.
pub(super) const MIN_COLUMN_CLUSTERS: usize = 2;
/// Size multiple above which a multi-cluster line is treated as one title line.
const LARGE_TEXT_BODY_SIZE_MULTIPLE: f64 = 1.3;
/// Body-size fraction below which a recovered run is superscript.
const SUPERSCRIPT_BODY_SIZE_RATIO: f64 = 0.8;

/// A text line extracted from a `PagedDocument` frame, preserving run-level info.
#[derive(Debug, Clone)]
pub(super) struct FrameLine {
    pub text: String,
    pub runs: Vec<Run>,
    pub x_clusters: Vec<XCluster>,
    pub page_idx: usize,
    pub y_pt: f64,
    pub all_math_font: bool,
}

#[derive(Debug, Clone)]
pub(super) struct XCluster {
    pub x_pt: f64,
    pub runs: Vec<Run>,
}

struct FrameTextItem {
    y: f64,
    x: f64,
    text: String,
    size_pt: f64,
    font_name: String,
}

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

pub(super) fn extract_lines_from_all_pages(
    paged: &PagedDocument,
    margins: super::super::page::MarginsPt,
) -> Vec<FrameLine> {
    let mut all_lines = Vec::new();

    let body_size = paged
        .pages()
        .first()
        .map_or(DEFAULT_RECOVERY_BODY_SIZE_PT, |p| {
            let mut items = Vec::new();
            collect_text_items_with_pos(&p.frame, &mut items);
            let mut sizes: HashMap<u32, usize> = HashMap::new();
            for item in &items {
                *sizes
                    .entry(super::super::page::pt_to_tenths(item.size_pt))
                    .or_default() += item.text.len();
            }
            // Tie-break on the smaller size so the detected body size is
            // deterministic (a bare `max_by_key` would pick whichever size the
            // HashMap iterated first when two tie on glyph count).
            super::super::stats::dominant_key(sizes.iter().map(|(size, count)| (size, *count)))
                .map_or(DEFAULT_RECOVERY_BODY_SIZE_PT, |size| {
                    f64::from(*size) / 10.0
                })
        });

    for (page_idx, page) in paged.pages().iter().enumerate() {
        let mut text_items = Vec::new();
        collect_text_items_with_pos(&page.frame, &mut text_items);

        // Drop header/footer chrome (running heads, page numbers) before it
        // becomes a candidate body line. We use the *same* margin boundary that
        // `detect_page_numbering`/`extract_footer` use to LOCATE the footer —
        // the document's resolved margins — so anything outside the body zone
        // is by definition margin content, and body content inside small
        // author margins (`#set page(margin: 1cm)`) is never thrown away.
        let (body_top, body_bottom) = super::super::page::find_body_zone(
            page.frame.width().to_pt(),
            page.frame.height().to_pt(),
            Some(margins.top),
            Some(margins.bottom),
        );
        text_items.retain(|item| body_top <= item.y && item.y <= body_bottom);

        let mut y_groups: BTreeMap<u64, Vec<&FrameTextItem>> = BTreeMap::new();
        for item in &text_items {
            let y_key = (item.y / LINE_Y_BUCKET_PT).round().to_bits();
            y_groups.entry(y_key).or_default().push(item);
        }

        for items in y_groups.values() {
            let page_width_pt = paged
                .pages()
                .first()
                .map_or(DEFAULT_PAGE_WIDTH_PT, |p| p.frame.width().to_pt());
            let max_font_size = items.iter().map(|i| i.size_pt).fold(0.0_f64, f64::max);
            let gap_threshold = (page_width_pt * CLUSTER_PAGE_WIDTH_RATIO)
                .max(MIN_CLUSTER_GAP_PT)
                .max(max_font_size * CLUSTER_FONT_SIZE_MULTIPLE);
            let raw_clusters = cluster_by_x(items, gap_threshold);

            let clusters = if raw_clusters.len() >= MIN_COLUMN_CLUSTERS {
                let max_size = raw_clusters
                    .iter()
                    .flat_map(|c| c.iter().map(|i| i.size_pt))
                    .fold(0.0_f64, f64::max);
                if max_size > body_size * LARGE_TEXT_BODY_SIZE_MULTIPLE {
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
                    let is_super = item.size_pt < body_size * SUPERSCRIPT_BODY_SIZE_RATIO;
                    let mut run = Run::new(&item.text);
                    run.superscript = is_super;
                    let half_pt = super::super::page::pt_to_half_pt(item.size_pt);
                    if half_pt != super::super::page::pt_to_half_pt(body_size) {
                        run.size_half_pt = Some(half_pt);
                    }
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
            let item_count = u32::try_from(items.len()).map_or(f64::from(u32::MAX), f64::from);
            let avg_y = items.iter().map(|i| i.y).sum::<f64>() / item_count;
            let all_math_font = items.iter().all(|i| i.font_name.contains("Math"));
            if !trimmed.is_empty() {
                all_lines.push(FrameLine {
                    text: trimmed,
                    runs: all_runs,
                    x_clusters,
                    page_idx,
                    y_pt: avg_y,
                    all_math_font,
                });
            }
        }
    }
    all_lines
}

fn collect_text_items_with_pos(frame: &Frame, items: &mut Vec<FrameTextItem>) {
    super::super::frames::visit_frame_items(frame, true, &mut |position, item| {
        if let FrameItem::Text(text_item) = item {
            let text = text_item.text.to_string();
            if !text.is_empty() {
                let font_name = text_item.font.info().family.clone();
                items.push(FrameTextItem {
                    y: position.y.to_pt(),
                    x: position.x.to_pt(),
                    text,
                    size_pt: text_item.size.to_pt(),
                    font_name,
                });
            }
        }
    });
}

pub(super) fn count_title_lines(paged_lines: &[FrameLine], doc: &Document) -> usize {
    let mut count = 0;
    for line in paged_lines {
        let is_heading = doc.body.elements.iter().any(|e| {
            if let BlockElement::Paragraph(p) = e
                && matches!(p.style, Some(ParagraphStyle::Heading(_)))
            {
                // Match on substantive heading text only. A heading's number prefix
                // ("A ", "1 ") is now a separate one-char run; matching on it would
                // misclassify any title-page line that merely starts with "A"/"1".
                p.text_runs().any(|r| {
                    r.text.trim().chars().count() >= MIN_HEADING_RUN_CHARS
                        && line.text.contains(&r.text)
                })
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
