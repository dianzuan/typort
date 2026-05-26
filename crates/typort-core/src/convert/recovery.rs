use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroUsize;

use typort_ooxml::document::{
    Alignment, BlockElement, Document, InlineElement, Paragraph, ParagraphStyle, Run,
};
use typst::introspection::Location;
use typst::layout::{Frame, FrameItem, PagedDocument, Point};
use typst_html::HtmlNode;

use super::{collect_block_tag_locations, strip_cjk_spaces_str, strip_visual_markers};

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

/// Recover content that exists in the `PagedDocument` but was lost from the `HtmlDocument` DOM.
pub(super) fn recover_missing_content(paged: &PagedDocument, doc: &mut Document) {
    let all_page_lines = extract_lines_from_all_pages(paged);
    if all_page_lines.is_empty() {
        return;
    }

    let title_line_count = count_title_lines(&all_page_lines, doc);
    let full_doc_text = extract_doc_text(doc);

    let mut exclude_text = extract_header_footer_text(doc);
    for footnote in &doc.footnotes {
        for inline in &footnote.content {
            if let InlineElement::Text(run) = inline {
                exclude_text.push_str(&run.text);
            }
        }
    }

    let mut missing = Vec::new();
    for (i, line) in all_page_lines.iter().enumerate() {
        if i < title_line_count {
            continue;
        }
        if line.text.chars().count() < 2 {
            continue;
        }
        if line.all_math_font {
            continue;
        }
        let line_normalized = strip_cjk_spaces_str(&line.text);
        let line_stripped = strip_visual_markers(&line.text);
        if full_doc_text.contains(&line.text)
            || full_doc_text.contains(&line_normalized)
            || full_doc_text.contains(&line_stripped)
            || exclude_text.contains(&line.text)
        {
            continue;
        }
        let words: Vec<&str> = line.text.split_whitespace().collect();
        if words.len() >= 2 {
            let sig_count = words.iter().filter(|w| w.chars().count() >= 3).count();
            let matched = words
                .iter()
                .filter(|w| w.chars().count() >= 3 && full_doc_text.contains(**w))
                .count();
            if sig_count >= 2 && matched * 2 > sig_count {
                continue;
            }
        }
        missing.push(line.clone());
    }

    if !missing.is_empty() {
        insert_missing_at_position(doc, &missing, &all_page_lines);
    }
}

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

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(super) fn extract_lines_from_all_pages(paged: &PagedDocument) -> Vec<FrameLine> {
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

    for (page_idx, page) in paged.pages.iter().enumerate() {
        let mut text_items = Vec::new();
        collect_text_items_with_pos(&page.frame, Point::zero(), &mut text_items);

        let mut y_groups: BTreeMap<i64, Vec<&FrameTextItem>> = BTreeMap::new();
        for item in &text_items {
            let y_key = (item.y / 8.0).round() as i64;
            y_groups.entry(y_key).or_default().push(item);
        }

        for items in y_groups.values() {
            let page_width_pt = paged
                .pages
                .first()
                .map_or(595.0, |p| p.frame.width().to_pt());
            let max_font_size = items.iter().map(|i| i.size_pt).fold(0.0_f64, f64::max);
            let gap_threshold = (page_width_pt * 0.06).max(20.0).max(max_font_size * 5.0);
            let raw_clusters = cluster_by_x(items, gap_threshold);

            let clusters = if raw_clusters.len() >= 2 {
                let max_size = raw_clusters
                    .iter()
                    .flat_map(|c| c.iter().map(|i| i.size_pt))
                    .fold(0.0_f64, f64::max);
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
            let avg_y = items.iter().map(|i| i.y).sum::<f64>() / items.len() as f64;
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

fn collect_text_items_with_pos(frame: &Frame, offset: Point, items: &mut Vec<FrameTextItem>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let text = text_item.text.to_string();
                if !text.is_empty() {
                    let font_name = text_item.font.info().family.clone();
                    items.push(FrameTextItem {
                        y: abs_y.to_pt(),
                        x: abs_x.to_pt(),
                        text,
                        size_pt: text_item.size.to_pt(),
                        font_name,
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

pub(super) fn extract_doc_text(doc: &Document) -> String {
    let mut text = String::new();
    for elem in &doc.body.elements {
        match elem {
            BlockElement::Paragraph(p) => {
                text.push_str(&p.full_text_content());
            }
            BlockElement::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for para in &cell.paragraphs {
                            text.push_str(&para.full_text_content());
                        }
                    }
                }
            }
            BlockElement::BibliographyBlock { paragraphs } => {
                for p in paragraphs {
                    text.push_str(&p.full_text_content());
                }
            }
        }
    }
    text
}

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

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pt_to_twips(pt: f64) -> u32 {
    (pt * 20.0).round().max(0.0) as u32
}

fn insert_missing_at_position(
    doc: &mut Document,
    missing_lines: &[FrameLine],
    all_page_lines: &[FrameLine],
) {
    let insert_idx = find_insert_position_by_y(doc, missing_lines, all_page_lines);

    let ps = &doc.page_settings;
    let content_width_twips = ps.width_twips.saturating_sub(ps.margin_left + ps.margin_right);

    let mut paragraphs: Vec<BlockElement> = Vec::new();
    for line in missing_lines {
        let mut para = Paragraph::new();
        para.suppress_indent = true;

        let has_large_gap = if line.x_clusters.len() >= 2 {
            // Estimate the font size for this line to determine gap threshold
            let max_font_size_pt = line
                .runs
                .iter()
                .map(|r| r.size_half_pt.unwrap_or(21) as f64 / 2.0)
                .fold(0.0_f64, f64::max);
            // A "real grid" gap should be at least 3em wide; h(0.5em) gaps are much smaller
            let small_gap_threshold = max_font_size_pt * 3.0;
            line.x_clusters.windows(2).any(|pair| {
                // Estimate end of left cluster from x position + char count * average char width
                let left_char_count: usize =
                    pair[0].runs.iter().map(|r| r.text.chars().count()).sum();
                let left_end =
                    pair[0].x_pt + left_char_count as f64 * max_font_size_pt;
                let gap = pair[1].x_pt - left_end;
                gap > small_gap_threshold
            })
        } else {
            false
        };
        let is_real_grid = line.x_clusters.len() >= 2
            && has_large_gap
            && line
                .x_clusters
                .iter()
                .all(|c| c.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() >= 3);
        if is_real_grid {
            let last_cluster = &line.x_clusters[line.x_clusters.len() - 1];
            let tab_pos = pt_to_twips(last_cluster.x_pt);
            let tab_stop = if tab_pos > 0 { tab_pos } else { content_width_twips };
            para.tab_stops.push(tab_stop);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
                    para.add_tab();
                }
                for run in &cluster.runs {
                    para.push_run(run.clone());
                }
            }
        } else if line.x_clusters.len() >= 2 {
            // Multiple clusters with small gaps — join with spaces, not tabs
            para.alignment = Some(Alignment::Center);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
                    let mut space_run = Run::new("\u{00a0}");
                    if let Some(first_run) = cluster.runs.first() {
                        space_run.size_half_pt = first_run.size_half_pt;
                        space_run.font_ascii = first_run.font_ascii.clone();
                        space_run.font_east_asia = first_run.font_east_asia.clone();
                    }
                    para.push_run(space_run);
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

    if !paragraphs.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(paragraphs);
        doc.body.elements.extend(tail);
    }
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
            && (line.y_pt - first_missing.y_pt).abs() < 2.0
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
    let search_prefix: String = text.chars().take(15).collect();
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

/// Build a mapping from document body element index to page number.
pub(super) fn build_element_page_map(
    children: &[HtmlNode],
    paged: &PagedDocument,
    total_elements: usize,
) -> Vec<usize> {
    if total_elements == 0 || paged.pages.is_empty() {
        return Vec::new();
    }

    let mut locs: Vec<Location> = Vec::new();
    collect_block_tag_locations(children, &mut locs);

    if locs.is_empty() {
        return vec![1; total_elements];
    }

    let tag_pages: Vec<usize> = locs
        .iter()
        .map(|loc| paged.introspector.page(*loc).get())
        .collect();

    let mut result = vec![1_usize; total_elements];
    let n_tags = tag_pages.len();

    for (elem_idx, slot) in result.iter_mut().enumerate() {
        let tag_idx = if n_tags >= total_elements {
            elem_idx.min(n_tags - 1)
        } else {
            (elem_idx * n_tags / total_elements).min(n_tags - 1)
        };
        *slot = tag_pages[tag_idx];
    }

    result
}

/// Collect page break locations by comparing element page numbers.
pub(super) fn collect_page_break_locations(
    children: &[HtmlNode],
    paged: &PagedDocument,
) -> HashSet<Location> {
    let mut locs: Vec<Location> = Vec::new();
    collect_block_tag_locations(children, &mut locs);

    if locs.is_empty() || paged.pages.len() < 2 {
        return HashSet::new();
    }

    let page_nums: Vec<NonZeroUsize> = locs
        .iter()
        .map(|loc| paged.introspector.page(*loc))
        .collect();

    let explicit_break_pages: HashSet<usize> = {
        use typst::foundations::{NativeElement, Selector};
        let selector = Selector::Elem(typst_library::layout::PagebreakElem::ELEM, None);
        let pagebreaks = paged.introspector.query(&selector);
        pagebreaks
            .iter()
            .filter_map(typst::foundations::Content::location)
            .map(|loc| paged.introspector.page(loc).get())
            .collect()
    };
    let has_any_pagebreak_elems = !explicit_break_pages.is_empty();

    let mut result = HashSet::new();
    for i in 1..locs.len() {
        if page_nums[i] > page_nums[i - 1] {
            let prev_page_num = page_nums[i - 1].get();
            if has_any_pagebreak_elems {
                let has_explicit = (prev_page_num..page_nums[i].get())
                    .any(|p| explicit_break_pages.contains(&p));
                if !has_explicit {
                    continue;
                }
            } else {
                let prev_page_idx = prev_page_num - 1;
                if prev_page_idx < paged.pages.len() {
                    let page = &paged.pages[prev_page_idx];
                    let page_height = page.frame.height().to_pt();
                    let content_y = find_max_content_y_in_frame(&page.frame, Point::zero());
                    if page_height > 0.0 && content_y >= page_height * 0.95 {
                        continue;
                    }
                }
            }
            result.insert(locs[i]);
        }
    }
    result
}

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

/// Detect horizontal line shapes and insert horizontal rule paragraphs.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub(super) fn insert_horizontal_rules_from_paged(
    paged: &PagedDocument,
    doc: &mut Document,
    element_page_map: &[usize],
) {
    let total_pages = paged.pages.len();
    if total_pages == 0 {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    let mut hrules: Vec<(usize, f64)> = Vec::new();

    for (page_idx, page) in paged.pages.iter().enumerate() {
        let page_width = page.frame.width().to_pt();
        let content_width = page_width * 0.6;

        let mut lines = Vec::new();
        collect_horizontal_lines(&page.frame, Point::zero(), content_width, &mut lines);

        for line_y in lines {
            hrules.push((page_idx + 1, line_y));
        }
    }

    if hrules.is_empty() {
        return;
    }

    hrules.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let full_doc_text = extract_doc_text(doc);

    for (page_num, _line_y) in &hrules {
        let insert_idx =
            if !element_page_map.is_empty() && element_page_map.len() == total_elements {
                element_page_map
                    .iter()
                    .position(|&p| p >= *page_num)
                    .unwrap_or(total_elements)
            } else {
                let normalized = f64::from(*page_num as u32 - 1) / f64::from(total_pages as u32);
                let idx = (normalized * total_elements as f64).round() as usize;
                idx.min(total_elements)
            };

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

fn collect_horizontal_lines(frame: &Frame, offset: Point, min_width: f64, lines: &mut Vec<f64>) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Shape(shape, _) => {
                if let typst::visualize::Geometry::Line(end_pt) = &shape.geometry {
                    let line_width = end_pt.x.to_pt().abs();
                    let line_height = end_pt.y.to_pt().abs();
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

/// Merge consecutive paragraphs whose text appears on the same visual line in
/// the paged output.  This fixes cases where Typst's HTML export splits inline
/// content (e.g. `super()` calls interleaved with author names in a `#for` loop)
/// into separate block-level elements that become separate Word paragraphs.
pub(super) fn merge_same_line_paragraphs(doc: &mut Document, paged: &PagedDocument) {
    let all_lines = extract_lines_from_all_pages(paged);
    if all_lines.is_empty() {
        return;
    }

    // Build text → y-position map from paged output
    let mut text_y_map: HashMap<String, f64> = HashMap::new();
    for line in &all_lines {
        for run in &line.runs {
            if !run.text.trim().is_empty() {
                text_y_map.insert(run.text.clone(), line.y_pt);
            }
        }
        text_y_map.insert(line.text.clone(), line.y_pt);
    }

    let mut i = 0;
    while i + 1 < doc.body.elements.len() {
        let should_merge = {
            let (left, right) = doc.body.elements.split_at(i + 1);
            let Some(BlockElement::Paragraph(p1)) = left.last() else {
                i += 1;
                continue;
            };
            let Some(BlockElement::Paragraph(p2)) = right.first() else {
                i += 1;
                continue;
            };

            // Both must be non-heading, non-list paragraphs
            if p1.style.is_some()
                || p2.style.is_some()
                || p1.list_info.is_some()
                || p2.list_info.is_some()
            {
                false
            } else {
                // Merge when p1 is all-superscript runs (split inline super() calls)
                // into p2 (the text paragraph that follows). This handles the
                // pattern where #for loop generates super() before author names.
                let p1_all_super = !p1.inlines.is_empty()
                    && p1.inlines.iter().all(|inl| matches!(
                        inl,
                        InlineElement::Text(r) if r.superscript || r.text.trim().is_empty()
                    ));
                p1_all_super
            }
        };

        if should_merge {
            // Remove the all-super p1 and prepend its inlines into p2
            let BlockElement::Paragraph(p1) =
                doc.body.elements.remove(i)
            else {
                unreachable!()
            };
            let BlockElement::Paragraph(p2) =
                &mut doc.body.elements[i]
            else {
                unreachable!()
            };
            let mut merged = p1.inlines;
            merged.extend(std::mem::take(&mut p2.inlines));
            p2.inlines = merged;
            // Don't increment i — check the merged paragraph against the next
        } else {
            i += 1;
        }
    }
}


fn find_y_for_text(
    text: &str,
    text_y_map: &HashMap<String, f64>,
    all_lines: &[FrameLine],
) -> Option<f64> {
    // Direct lookup
    if let Some(&y) = text_y_map.get(text) {
        return Some(y);
    }
    // Substring match: find the first line whose text contains this text
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        for line in all_lines {
            if line.text.contains(trimmed) || trimmed.contains(&line.text) {
                return Some(line.y_pt);
            }
            for run in &line.runs {
                if run.text.contains(trimmed) || trimmed.contains(run.text.as_str()) {
                    return Some(line.y_pt);
                }
            }
        }
    }
    None
}
