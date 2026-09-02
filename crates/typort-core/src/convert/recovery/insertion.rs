//! Geometric placement and construction of recovered content.

use crate::convert::page::DEFAULT_BODY_SIZE_HALF_PT;
use typort_ooxml::document::{
    Alignment, BlockElement, Document, InlineElement, Paragraph, ParagraphStyle, Run,
};

use super::lines::{FrameLine, MIN_COLUMN_CLUSTERS};

/// Maximum combined character count for merging ordinary centered lines.
const MAX_CENTERED_MERGE_CHARS: usize = 60;
/// Font-size multiple bounding consecutive recovered-line baselines.
const MAX_CONTIGUOUS_LINE_GAP_MULTIPLE: f64 = 2.0;
/// Font-size multiple defining a visually large inter-cluster gap.
const LARGE_CLUSTER_GAP_MULTIPLE: f64 = 3.0;
/// Y-position tolerance in points when locating a recovered line.
const RECOVERED_LINE_Y_TOLERANCE_PT: f64 = 2.0;
/// Prefix length used to match paged text back to emitted elements.
const ELEMENT_TEXT_PREFIX_CHARS: usize = 15;
/// Minimum characters per recovered grid column.
const MIN_RECOVERED_GRID_COLUMN_CHARS: usize = 3;

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

pub(super) fn insert_missing_at_position(
    doc: &mut Document,
    missing_lines: &[FrameLine],
    all_page_lines: &[FrameLine],
) {
    let insert_idx = find_insert_position_by_y(doc, missing_lines, all_page_lines);

    let ps = &doc.page_settings;
    let content_width_twips = ps
        .width_twips
        .saturating_sub(ps.margin_left + ps.margin_right);

    let mut paragraphs: Vec<BlockElement> = Vec::new();
    for line in missing_lines {
        let mut para = Paragraph::new();
        para.suppress_indent = true;
        // Remember the real page this line came from (1-based), so the
        // element→page map uses it directly instead of interpolating.
        para.page_from_paged = Some(line.page_idx + 1);

        let has_large_gap = line_has_large_cluster_gap(line);
        let is_real_grid = line.x_clusters.len() >= MIN_COLUMN_CLUSTERS
            && has_large_gap
            && line.x_clusters.iter().all(|c| {
                c.runs.iter().map(|r| r.text.chars().count()).sum::<usize>()
                    >= MIN_RECOVERED_GRID_COLUMN_CHARS
            });
        if is_real_grid {
            let last_cluster = &line.x_clusters[line.x_clusters.len() - 1];
            let tab_pos = super::super::page::pt_to_twips(last_cluster.x_pt);
            let tab_stop = if tab_pos > 0 {
                tab_pos
            } else {
                content_width_twips
            };
            para.tab_stops.push(tab_stop);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
                    para.add_tab();
                }
                for run in &cluster.runs {
                    para.push_run(run.clone());
                }
            }
        } else if line.x_clusters.len() >= MIN_COLUMN_CLUSTERS {
            // Multiple clusters with small gaps — join with spaces, not tabs. Insert
            // an NBSP at the boundary only when neither side already carries a
            // whitespace character. Clusters recovered from paged text items
            // sometimes already have a source space baked into a run's text (e.g.
            // "上海 200433" — the space survives as the leading char of the next
            // cluster's first run); unconditionally inserting NBSP there would
            // double the visible gap. When neither side has whitespace, the gap is
            // purely visual (no space character in the source) and still needs the
            // NBSP to render.
            para.alignment = Some(Alignment::Center);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
                    let prev_has_trailing_space = line.x_clusters[idx - 1]
                        .runs
                        .last()
                        .is_some_and(|r| r.text.ends_with(char::is_whitespace));
                    let next_has_leading_space = cluster
                        .runs
                        .first()
                        .is_some_and(|r| r.text.starts_with(char::is_whitespace));
                    if !prev_has_trailing_space && !next_has_leading_space {
                        let mut space_run = Run::new("\u{00a0}");
                        if let Some(first_run) = cluster.runs.first() {
                            space_run.size_half_pt = first_run.size_half_pt;
                            space_run.font_ascii.clone_from(&first_run.font_ascii);
                            space_run
                                .font_east_asia
                                .clone_from(&first_run.font_east_asia);
                        }
                        para.push_run(space_run);
                    }
                }
                for run in &cluster.runs {
                    para.push_run(run.clone());
                }
            }
        } else {
            para.alignment = Some(Alignment::Center);
            for run in &line.runs {
                para.push_run(run.clone());
            }
        }
        paragraphs.push(BlockElement::Paragraph(para));
    }

    let merged = merge_centered_paragraphs(paragraphs, missing_lines);

    if !merged.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(merged);
        doc.body.elements.extend(tail);
    }
}

fn merge_centered_paragraphs(
    paragraphs: Vec<BlockElement>,
    source_lines: &[FrameLine],
) -> Vec<BlockElement> {
    let mut merged = Vec::new();
    let mut previous_line = None;
    for (element, current_line) in paragraphs.into_iter().zip(source_lines) {
        let should_merge =
            if let (Some(BlockElement::Paragraph(previous)), BlockElement::Paragraph(current)) =
                (merged.last(), &element)
            {
                let previous_size = first_text_size(previous);
                let current_size = first_text_size(current);
                let combined_length = previous.text_content().chars().count()
                    + current.text_content().chars().count();
                let same_non_default_size =
                    previous_size == current_size && previous_size.is_some();
                matches!(previous.alignment, Some(Alignment::Center))
                    && matches!(current.alignment, Some(Alignment::Center))
                    && previous_size == current_size
                    && (combined_length < MAX_CENTERED_MERGE_CHARS || same_non_default_size)
                    && previous_line
                        .is_some_and(|line| recovered_lines_are_contiguous(line, current_line))
            } else {
                false
            };

        if should_merge {
            let BlockElement::Paragraph(current) = element else {
                unreachable!()
            };
            let Some(BlockElement::Paragraph(previous)) = merged.last_mut() else {
                unreachable!()
            };
            previous.inlines.extend(current.inlines);
        } else {
            merged.push(element);
        }
        previous_line = Some(current_line);
    }
    merged
}

fn recovered_lines_are_contiguous(previous: &FrameLine, current: &FrameLine) -> bool {
    if previous.page_idx != current.page_idx {
        return false;
    }
    let font_size_pt = previous
        .runs
        .iter()
        .chain(&current.runs)
        .map(|run| f64::from(run.size_half_pt.unwrap_or(DEFAULT_BODY_SIZE_HALF_PT)) / 2.0)
        .fold(0.0_f64, f64::max);
    let vertical_gap = current.y_pt - previous.y_pt;
    vertical_gap >= 0.0 && vertical_gap <= font_size_pt * MAX_CONTIGUOUS_LINE_GAP_MULTIPLE
}

fn first_text_size(paragraph: &Paragraph) -> Option<u32> {
    paragraph.inlines.iter().find_map(|inline| {
        if let InlineElement::Text(run) = inline {
            run.size_half_pt
        } else {
            None
        }
    })
}

fn line_has_large_cluster_gap(line: &FrameLine) -> bool {
    if line.x_clusters.len() < MIN_COLUMN_CLUSTERS {
        return false;
    }
    let max_font_size_pt = line
        .runs
        .iter()
        .map(|run| f64::from(run.size_half_pt.unwrap_or(DEFAULT_BODY_SIZE_HALF_PT)) / 2.0)
        .fold(0.0_f64, f64::max);
    let gap_threshold = max_font_size_pt * LARGE_CLUSTER_GAP_MULTIPLE;
    line.x_clusters.windows(2).any(|pair| {
        let left_char_count: usize = pair[0]
            .runs
            .iter()
            .map(|run| run.text.chars().count())
            .sum();
        let left_char_count = u32::try_from(left_char_count).map_or(f64::from(u32::MAX), f64::from);
        let left_end = pair[0].x_pt + left_char_count * max_font_size_pt;
        pair[1].x_pt - left_end > gap_threshold
    })
}

fn find_insert_position_by_y(
    doc: &Document,
    missing_lines: &[FrameLine],
    all_page_lines: &[FrameLine],
) -> usize {
    let Some(first_missing) = missing_lines.first() else {
        return find_title_section_end(doc);
    };

    let missing_idx = all_page_lines.iter().position(|line| {
        line.text == first_missing.text
            && line.page_idx == first_missing.page_idx
            && (line.y_pt - first_missing.y_pt).abs() < RECOVERED_LINE_Y_TOLERANCE_PT
    });

    if let Some(idx) = missing_idx {
        for j in (0..idx).rev() {
            let candidate = &all_page_lines[j];
            if let Some(elem_idx) = find_element_by_text(doc, &candidate.text) {
                return elem_idx + 1;
            }
        }
    }

    find_title_section_end(doc)
}

fn find_element_by_text(doc: &Document, text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let search_prefix: String = text.chars().take(ELEMENT_TEXT_PREFIX_CHARS).collect();
    for (i, elem) in doc.body.elements.iter().enumerate() {
        let elem_text = match elem {
            BlockElement::Paragraph(p) => p.text_content(),
            BlockElement::Table(_) | BlockElement::BibliographyBlock { .. } => continue,
        };
        if elem_text.contains(&search_prefix) {
            return Some(i);
        }
    }
    None
}
