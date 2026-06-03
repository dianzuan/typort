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
#[allow(clippy::too_many_lines)]
pub(super) fn recover_missing_content(paged: &PagedDocument, doc: &mut Document) {
    let all_page_lines = extract_lines_from_all_pages(paged);
    if all_page_lines.is_empty() {
        return;
    }

    let title_line_count = count_title_lines(&all_page_lines, doc);
    let full_doc_text = extract_doc_text(doc);
    let full_doc_text_nospace = strip_math_italic(&full_doc_text).replace(' ', "");
    // CJK-only projection of the whole document, used to recognize paged lines
    // whose prose is already present but broken up by interleaved OMML math,
    // superscript citation marks, or heading numbers (which defeat the byte-level
    // substring/word/fragment checks below).
    let doc_cjk: String = full_doc_text
        .chars()
        .filter(|c| is_cjk_ideograph(*c))
        .collect();

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
        // Skip very short lines ending with sentence-final punctuation —
        // these are fragments of sentence tails from line-wrapped paragraphs
        // that are already present in the full text.
        if line.text.chars().count() <= 5
            && line.text.trim().ends_with(['。', '.', '；', '！', '？'])
        {
            continue;
        }
        if line.all_math_font {
            continue;
        }
        // Skip short lines with math symbols — these are equation fragments
        // that are already represented as OMML in the document.
        {
            let math_chars = line
                .text
                .chars()
                .filter(|c| {
                    ('\u{1D400}'..='\u{1D7FF}').contains(c)
                    || ('\u{2200}'..='\u{22FF}').contains(c) // math operators (∗, ∑, etc.)
                    || *c == '>' || *c == '<' || *c == '≥' || *c == '≤'
                })
                .count();
            let total = line.text.chars().count();
            // Short lines with ANY math: skip if under 8 chars total
            if total < 8 && math_chars > 0 {
                continue;
            }
            if total > 0 && math_chars * 4 > total {
                continue;
            }
        }
        let line_normalized = strip_cjk_spaces_str(&line.text);
        let line_demath = strip_math_italic(&line.text);
        let line_stripped = strip_visual_markers(&line.text);
        let line_no_numbering = strip_heading_numbering(&line.text);
        let line_demath_nospace = line_demath.replace(' ', "");
        // Short lines (< 6 chars) can match as false-positive substrings in
        // longer paragraphs (e.g. author name "作者甲" inside author bio).
        // For these, only check per-paragraph exact match, not full-text substring.
        let short_line = line.text.chars().count() < 6;
        let short_line_exact = short_line
            && doc.body.elements.iter().any(|e| {
                if let BlockElement::Paragraph(p) = e {
                    let t = p.text_content();
                    t.trim() == line.text.trim()
                } else {
                    false
                }
            });
        if short_line_exact
            || (!short_line
                && (full_doc_text.contains(&line.text)
                    || full_doc_text.contains(&line_normalized)
                    || full_doc_text.contains(&line_stripped)
                    || full_doc_text.contains(&line_demath)
                    || full_doc_text_nospace.contains(&line_demath_nospace)))
            || (!line_no_numbering.is_empty() && full_doc_text.contains(&line_no_numbering))
            || exclude_text.contains(&line.text)
        {
            continue;
        }
        // Fuzzy match: for lines containing math (short tokens mixed with text),
        // check if significant text fragments (3+ chars) are already in the doc.
        // Also check with math italic stripped to handle Unicode math ↔ ASCII OMML.
        let words: Vec<&str> = line.text.split_whitespace().collect();
        if words.len() >= 2 {
            let sig_count = words.iter().filter(|w| w.chars().count() >= 3).count();
            let matched = words
                .iter()
                .filter(|w| {
                    let wlen = w.chars().count();
                    if wlen < 3 {
                        return false;
                    }
                    full_doc_text.contains(*w) || {
                        let dw = strip_math_italic(w);
                        full_doc_text.contains(dw.as_str())
                            || full_doc_text_nospace.contains(dw.replace(' ', "").as_str())
                    }
                })
                .count();
            // Treat the line as already-present when a majority of significant
            // words are found elsewhere. This majority rule (not all-or-nothing)
            // is required for math-interleaved lines: a theorem/proof/footnote
            // line whose prose is already in the DOM as an OMML paragraph will
            // have its math tokens (𝑓, [𝑎,𝑏], 𝛼) fail to byte-match the OMML, so
            // an `== sig_count` rule would re-insert it as a flat-text duplicate.
            // An empirical before/after scan over all fixtures confirmed the
            // stricter rule duplicated content in 21 documents.
            if sig_count >= 2 && matched * 2 > sig_count {
                continue;
            }
        }
        // If the line has any CJK-heavy word (4+ ideographs) that appears in
        // doc text, the line is likely already present (mixed with OMML math).
        if words.iter().any(|w| {
            let cjk_count = w
                .chars()
                .filter(|c| matches!(*c, '\u{4E00}'..='\u{9FFF}'))
                .count();
            cjk_count >= 4 && full_doc_text.contains(*w)
        }) {
            continue;
        }
        // CJK fragment matching: extract CJK ideograph substrings (8+ chars)
        // and check against doc text.
        let long_frags: Vec<String> = extract_cjk_fragments(&line.text, 8);
        if long_frags
            .iter()
            .any(|f| full_doc_text.contains(f.as_str()))
        {
            continue;
        }
        // For shorter fragments (4+ chars), require majority to match.
        let short_frags: Vec<String> = extract_cjk_fragments(&line.text, 4);
        if short_frags.len() >= 2 {
            let frag_matched = short_frags
                .iter()
                .filter(|f| full_doc_text.contains(f.as_str()))
                .count();
            if frag_matched * 2 > short_frags.len() {
                continue;
            }
        }
        // A line that is only citation/footnote markers (e.g. "[1][2][5]") holds
        // no recoverable prose — those marks are already emitted as citations.
        if strip_citation_markers(&line.text).trim().is_empty() {
            continue;
        }
        // CJK-projection match: strip citation markers and a leading heading
        // number, then project to CJK ideographs and treat the line as already
        // present when that projection is a contiguous substring of the document.
        // This catches body lines the checks above miss because their prose is
        // split by OMML math / superscript citations (which never byte-match).
        {
            let norm = strip_leading_heading_number(&strip_citation_markers(&line.text));
            let line_cjk: String = norm.chars().filter(|c| is_cjk_ideograph(*c)).collect();
            let cjk_len = line_cjk.chars().count();
            let has_math = line.text.chars().any(|c| {
                ('\u{1D400}'..='\u{1D7FF}').contains(&c) || ('\u{2200}'..='\u{22FF}').contains(&c)
            });
            if (cjk_len >= 6 || (cjk_len >= 2 && has_math)) && doc_cjk.contains(&line_cjk) {
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
        // Tie-break on the smaller size so the detected body size is
        // deterministic (a bare `max_by_key` would pick whichever size the
        // HashMap iterated first when two tie on glyph count).
        sizes
            .into_iter()
            .max_by_key(|(s, c)| (*c, std::cmp::Reverse(*s)))
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
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let half_pt = (item.size_pt * 2.0).round() as u32;
                    #[allow(clippy::cast_sign_loss)]
                    if half_pt != (body_size * 2.0).round().max(0.0) as u32 {
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

/// Map Unicode Mathematical Italic/Bold/Script characters to ASCII equivalents.
/// Paged output renders math as Unicode math italic (U+1D400-U+1D7FF) while
/// OMML stores them as plain ASCII with formatting attributes.
#[allow(clippy::cast_possible_truncation)]
fn strip_math_italic(text: &str) -> String {
    text.chars()
        .map(|c| {
            let cp = c as u32;
            match cp {
                // Math italic small a-z: U+1D44E-U+1D467
                0x1D44E..=0x1D467 => (b'a' + (cp - 0x1D44E) as u8) as char,
                // Math italic capital A-Z: U+1D434-U+1D44D
                0x1D434..=0x1D44D => (b'A' + (cp - 0x1D434) as u8) as char,
                // Math bold small a-z: U+1D41A-U+1D433
                0x1D41A..=0x1D433 => (b'a' + (cp - 0x1D41A) as u8) as char,
                // Math bold capital A-Z: U+1D400-U+1D419
                0x1D400..=0x1D419 => (b'A' + (cp - 0x1D400) as u8) as char,
                // Math bold italic small a-z: U+1D482-U+1D49B
                0x1D482..=0x1D49B => (b'a' + (cp - 0x1D482) as u8) as char,
                // Math bold italic capital A-Z: U+1D468-U+1D481
                0x1D468..=0x1D481 => (b'A' + (cp - 0x1D468) as u8) as char,
                // Math italic h: U+210E
                0x210E => 'h',
                _ => c,
            }
        })
        .collect()
}

/// Strip common Chinese heading numbering prefixes for fuzzy matching.
/// Patterns: "一、", "（一）", "1. ", "第一章 " etc.
fn strip_heading_numbering(text: &str) -> String {
    let trimmed = text.trim();
    // "一、引言" → "引言", "二、文献综述" → "文献综述"
    let cn_nums = [
        "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四",
        "十五",
    ];
    for num in &cn_nums {
        let prefix = format!("{num}、");
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return rest.trim().to_string();
        }
        let prefix = format!("（{num}）");
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return rest.trim().to_string();
        }
        // With space after prefix: "一、 引言"
        let prefix = format!("{num}、 ");
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return rest.trim().to_string();
        }
        let prefix = format!("（{num}） ");
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return rest.trim().to_string();
        }
    }
    // Arabic number prefixes: "1. ", "2. ", "1 ", etc.
    if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    String::new()
}

/// Whether `c` is a CJK ideograph (the ranges used for projection/fragments).
fn is_cjk_ideograph(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}

/// Remove inline citation/footnote markers like `[12]`, `[1,2]` or `[1-3]` from a
/// line. Such marks are already emitted as citations/footnote refs, so a paged
/// line is not "missing" merely because it carries (or is made entirely of) them.
fn strip_citation_markers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(rel) = chars[i + 1..].iter().position(|c| *c == ']')
        {
            let inner = &chars[i + 1..i + 1 + rel];
            if !inner.is_empty()
                && inner
                    .iter()
                    .all(|c| c.is_ascii_digit() || matches!(c, ',' | '，' | ' ' | '-' | '–'))
            {
                i = i + 1 + rel + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Strip a leading heading-number prefix ("三、", "(三)" or "（三）"), returning the
/// remainder. Unlike [`strip_heading_numbering`], the input is returned unchanged
/// (not emptied) when there is no such prefix, so it is safe on ordinary prose.
fn strip_leading_heading_number(text: &str) -> String {
    const CN_NUMS: [&str; 15] = [
        "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四",
        "十五",
    ];
    let t = text.trim_start();
    for num in CN_NUMS {
        for prefix in [format!("{num}、"), format!("({num})"), format!("（{num}）")] {
            if let Some(rest) = t.strip_prefix(&prefix) {
                return rest.trim_start().to_string();
            }
        }
    }
    t.to_string()
}

/// Extract contiguous CJK ideograph runs of at least `min_len` characters.
/// Uses only CJK Unified Ideographs (not fullwidth punctuation) so that
/// fragments like "被主流接受" match regardless of surrounding punctuation.
fn extract_cjk_fragments(text: &str, min_len: usize) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}') {
            current.push(c);
        } else if current.chars().count() >= min_len {
            fragments.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.chars().count() >= min_len {
        fragments.push(current);
    }
    fragments
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

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn insert_missing_at_position(
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

        let has_large_gap = if line.x_clusters.len() >= 2 {
            // Estimate the font size for this line to determine gap threshold
            let max_font_size_pt = line
                .runs
                .iter()
                .map(|r| f64::from(r.size_half_pt.unwrap_or(21)) / 2.0)
                .fold(0.0_f64, f64::max);
            // A "real grid" gap should be at least 3em wide; h(0.5em) gaps are much smaller
            let small_gap_threshold = max_font_size_pt * 3.0;
            line.x_clusters.windows(2).any(|pair| {
                // Estimate end of left cluster from x position + char count * average char width
                let left_char_count: usize =
                    pair[0].runs.iter().map(|r| r.text.chars().count()).sum();
                let left_end = pair[0].x_pt + left_char_count as f64 * max_font_size_pt;
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
        } else if line.x_clusters.len() >= 2 {
            // Multiple clusters with small gaps — join with spaces, not tabs
            para.alignment = Some(Alignment::Center);
            for (idx, cluster) in line.x_clusters.iter().enumerate() {
                if idx > 0 {
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

    // Merge consecutive centered paragraphs that have the same font size
    // (likely a single title wrapping across multiple rendered lines).
    let mut merged: Vec<BlockElement> = Vec::new();
    for elem in paragraphs {
        let should_merge =
            if let (Some(BlockElement::Paragraph(prev)), BlockElement::Paragraph(curr)) =
                (merged.last(), &elem)
            {
                let prev_center = matches!(prev.alignment, Some(Alignment::Center));
                let curr_center = matches!(curr.alignment, Some(Alignment::Center));
                let prev_size = prev.inlines.iter().find_map(|i| {
                    if let InlineElement::Text(r) = i {
                        r.size_half_pt
                    } else {
                        None
                    }
                });
                let curr_size = curr.inlines.iter().find_map(|i| {
                    if let InlineElement::Text(r) = i {
                        r.size_half_pt
                    } else {
                        None
                    }
                });
                // Only merge short centered paragraphs (wrapped title lines).
                // Don't merge if current starts with "（" or "(" (affiliation).
                let prev_text = prev.text_content();
                let curr_text = curr.text_content();
                let combined_len = prev_text.chars().count() + curr_text.chars().count();
                let combined_short = combined_len < 60;
                let curr_is_affiliation = curr_text.starts_with('（') || curr_text.starts_with('(');
                // Don't merge very short items (likely author names, not title fragments)
                let curr_too_short = curr_text.chars().count() <= 5;
                // Merge if both are short, OR if both have the same non-default
                // font size (i.e. they're from the same styled block like a title)
                let same_styled = prev_size == curr_size && prev_size.is_some();
                prev_center
                    && curr_center
                    && prev_size == curr_size
                    && (combined_short || same_styled)
                    && !curr_is_affiliation
                    && !curr_too_short
            } else {
                false
            };

        if should_merge {
            let BlockElement::Paragraph(curr) = elem else {
                unreachable!()
            };
            let Some(BlockElement::Paragraph(prev)) = merged.last_mut() else {
                unreachable!()
            };
            prev.inlines.extend(curr.inlines);
        } else {
            merged.push(elem);
        }
    }

    if !merged.is_empty() {
        let tail = doc.body.elements.split_off(insert_idx);
        doc.body.elements.extend(merged);
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
///
/// Recovered paragraphs carry their real page in `page_from_paged` and use it
/// directly. Remaining elements (the ones that came from HTML block tags) are
/// mapped from those tags' introspector pages; only as a last resort — when tag
/// count and element count disagree — is a proportional interpolation used.
pub(super) fn build_element_page_map(
    doc: &Document,
    children: &[HtmlNode],
    paged: &PagedDocument,
) -> Vec<usize> {
    let total_elements = doc.body.elements.len();
    if total_elements == 0 || paged.pages.is_empty() {
        return Vec::new();
    }

    let mut locs: Vec<Location> = Vec::new();
    collect_block_tag_locations(children, &mut locs);

    if locs.is_empty() {
        // No tag positions at all: use recovered pages where known, else page 1.
        return doc
            .body
            .elements
            .iter()
            .map(|el| match el {
                BlockElement::Paragraph(p) => p.page_from_paged.unwrap_or(1),
                _ => 1,
            })
            .collect();
    }

    let tag_pages: Vec<usize> = locs
        .iter()
        .map(|loc| paged.introspector.page(*loc).get())
        .collect();

    let n_tags = tag_pages.len();
    let mut result = vec![1_usize; total_elements];
    // Walk tag-derived pages and recovered pages in parallel: elements with a
    // known recovered page take it; the rest consume the next tag page in order.
    let mut tag_cursor = 0;
    for (elem_idx, slot) in result.iter_mut().enumerate() {
        if let BlockElement::Paragraph(p) = &doc.body.elements[elem_idx]
            && let Some(page) = p.page_from_paged
        {
            *slot = page;
            continue;
        }
        let tag_idx = if n_tags >= total_elements {
            elem_idx.min(n_tags - 1)
        } else {
            tag_cursor.min(n_tags - 1)
        };
        *slot = tag_pages[tag_idx];
        tag_cursor += 1;
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
                let has_explicit =
                    (prev_page_num..page_nums[i].get()).any(|p| explicit_break_pages.contains(&p));
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
        let insert_idx = if !element_page_map.is_empty() && element_page_map.len() == total_elements
        {
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
pub(super) fn merge_same_line_paragraphs(doc: &mut Document, _paged: &PagedDocument) {
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
                !p1.inlines.is_empty()
                    && p1.inlines.iter().all(|inl| {
                        matches!(
                            inl,
                            InlineElement::Text(r) if r.superscript || r.text.trim().is_empty()
                        )
                    })
            }
        };

        if should_merge {
            // Remove the all-super p1 and prepend its inlines into p2
            let BlockElement::Paragraph(p1) = doc.body.elements.remove(i) else {
                unreachable!()
            };
            let BlockElement::Paragraph(p2) = &mut doc.body.elements[i] else {
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
