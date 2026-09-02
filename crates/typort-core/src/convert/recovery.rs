use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;

use typort_ooxml::document::{
    Alignment, BlockElement, CellContent, Document, InlineElement, Paragraph, ParagraphStyle, Run,
    Table, TableBorders,
};
use typst::foundations::NativeElement;
use typst::introspection::{Introspector, Location};
use typst::layout::{Frame, FrameItem};
use typst_html::HtmlNode;
use typst_layout::PagedDocument;

use super::{collect_block_tag_locations, strip_cjk_spaces_str, strip_visual_markers};

// Recovery heuristic thresholds. Keep every tuning knob in this block.

/// Minimum rendered line length eligible for recovery.
const MIN_RECOVERED_LINE_CHARS: usize = 2;
/// Lines shorter than this use exact short-line matching only.
const SHORT_LINE_CHAR_BOUNDARY: usize = 6;
/// Minimum word count needed for majority word matching.
const MIN_LINE_WORDS: usize = 2;
/// Minimum word length included in majority matching.
const MIN_SIGNIFICANT_WORD_CHARS: usize = 3;
/// Minimum significant-word count needed for a majority decision.
const MIN_SIGNIFICANT_WORDS: usize = 2;
/// Minimum CJK characters in a word used for direct matching.
const MIN_CJK_WORD_CHARS: usize = 4;
/// Minimum CJK characters in a strong fragment match.
const MIN_LONG_CJK_FRAGMENT_CHARS: usize = 8;
/// Minimum CJK characters in a fragment used for majority matching.
const MIN_SHORT_CJK_FRAGMENT_CHARS: usize = 4;
/// Minimum short CJK fragments needed for a majority decision.
const MIN_SHORT_CJK_FRAGMENTS: usize = 2;
/// Minimum CJK projection length for matching a non-math line.
const MIN_CJK_PROJECTION_CHARS: usize = 6;
/// Minimum CJK projection length for matching a line containing math.
const MIN_MATH_CJK_PROJECTION_CHARS: usize = 2;
/// Minimum substantive emitted-heading run length used for title detection.
const MIN_HEADING_RUN_CHARS: usize = 2;
/// Minimum normalized table-cell text retained for matching.
const MIN_TABLE_CELL_CHARS: usize = 2;
/// Minimum recovered table-column text retained for matching.
const MIN_TABLE_COLUMN_CHARS: usize = 3;
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
const MIN_COLUMN_CLUSTERS: usize = 2;
/// Size multiple above which a multi-cluster line is treated as one title line.
const LARGE_TEXT_BODY_SIZE_MULTIPLE: f64 = 1.3;
/// Body-size fraction below which a recovered run is superscript.
const SUPERSCRIPT_BODY_SIZE_RATIO: f64 = 0.8;
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
/// Page-width fraction a line must span to be a document rule candidate.
const HORIZONTAL_RULE_PAGE_WIDTH_RATIO: f64 = 0.6;
/// Minimum document-text bytes required before inserting a recovered rule.
const MIN_RULE_DOCUMENT_TEXT_BYTES: usize = 10;
/// Maximum vertical extent in points for a horizontal document rule.
const MAX_HORIZONTAL_RULE_HEIGHT_PT: f64 = 5.0;
/// Minimum Word border thickness in eighth-points.
const MIN_TABLE_RULE_EIGHTH_PT: u32 = 2;
/// Maximum minor-axis extent in points for a table rule.
const TABLE_RULE_AXIS_TOLERANCE_PT: f64 = 0.5;
/// Minimum horizontal table-rule length in points.
const MIN_HORIZONTAL_TABLE_RULE_LENGTH_PT: f64 = 40.0;
/// Minimum vertical table-rule length in points.
const MIN_VERTICAL_TABLE_RULE_LENGTH_PT: f64 = 8.0;
/// Minimum characters per recovered grid column.
const MIN_RECOVERED_GRID_COLUMN_CHARS: usize = 3;
/// Multiplier used to express strict or inclusive majority comparisons.
const MAJORITY_SCALE: usize = 2;
/// Half-point fallback for recovered runs without an explicit rendered size.
const DEFAULT_RECOVERED_RUN_SIZE_HALF_PT: u32 = 21;

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
    let margins = super::page::MarginsPt::from_settings(&doc.page_settings);
    let all_page_lines = extract_lines_from_all_pages(paged, margins);
    if all_page_lines.is_empty() {
        return;
    }

    let title_line_count = count_title_lines(&all_page_lines, doc);
    let mut full_doc_text = extract_doc_text(doc);
    // Footnote bodies are real footnotes in the model (in footnotes.xml), not body
    // content. Fold their text into the dedup corpus so the page-bottom footnote
    // zone is never re-scraped into orphan body paragraphs. (They are also kept in
    // `exclude_text` below for the exact-line path.)
    append_footnote_text(doc, &mut full_doc_text);
    let full_doc_text_nospace = strip_math_italic(&full_doc_text).replace(' ', "");
    // CJK-only projection of the whole document, used to recognize paged lines
    // whose prose is already present but broken up by interleaved OMML math,
    // superscript citation marks, or heading numbers (which defeat the byte-level
    // substring/word/fragment checks below).
    let doc_cjk: String = full_doc_text
        .chars()
        .filter(|c| is_cjk_ideograph(*c))
        .collect();
    // Whitespace-stripped cell texts of every real table. A recovered
    // multi-column line whose columns are substrings of these is a re-scraped
    // table row that text-dedup can miss when a narrow column wrapped a cell
    // (the wrap truncates the cell so it no longer substring-matches the model).
    let table_cell_texts = collect_table_cell_texts(doc);
    // Whitespace-cancelled text of every emitted heading. A paged heading line
    // carries Typst's own computed number ("三、", "1.1", "十六、", …) exactly as the
    // emitted heading paragraph does — the number comes from the semantic
    // `HeadingElem.numbers`, prepended at emission — so an exact match with all
    // whitespace removed dedups a re-scraped heading regardless of numbering scheme
    // or language, with no hardcoded numeral table. This is the short-line
    // counterpart of the `full_doc_text_nospace` check below, which the `!short_line`
    // gate skips (a short heading numbered outside any fixed table would otherwise be
    // re-injected as a duplicate orphan).
    let heading_texts_nospace: Vec<String> = doc
        .body
        .elements
        .iter()
        .filter_map(|e| match e {
            BlockElement::Paragraph(p) if matches!(p.style, Some(ParagraphStyle::Heading(_))) => {
                let t = cancel_whitespace(&strip_math_italic(&p.text_content()));
                (!t.is_empty()).then_some(t)
            }
            _ => None,
        })
        .collect();

    let mut exclude_text = extract_header_footer_text(doc);
    append_footnote_text(doc, &mut exclude_text);

    let mut missing = Vec::new();
    for (i, line) in all_page_lines.iter().enumerate() {
        if i < title_line_count {
            continue;
        }
        if line.text.chars().count() < MIN_RECOVERED_LINE_CHARS {
            continue;
        }
        if line.all_math_font {
            continue;
        }
        if line_matches_existing_text(
            line,
            doc,
            &full_doc_text,
            &full_doc_text_nospace,
            &heading_texts_nospace,
            &exclude_text,
        ) {
            continue;
        }
        if line_words_are_already_emitted(line, &full_doc_text, &full_doc_text_nospace) {
            continue;
        }
        if line_cjk_is_already_emitted(line, &full_doc_text, &doc_cjk) {
            continue;
        }
        // A recovered *multi-column* line whose columns are each (a substring of)
        // a real table cell is a re-scraped table row. This catches the case the
        // text checks above miss: a narrow column wraps a cell, truncating it so
        // it no longer substring-matches the model. Scoped to lines with ≥2 x
        // clusters and only when the model actually has a table, so genuine grid
        // content (no `BlockElement::Table`) is still recovered.
        if !table_cell_texts.is_empty()
            && line.x_clusters.len() >= MIN_COLUMN_CLUSTERS
            && line_matches_table_cells(line, &table_cell_texts)
        {
            continue;
        }
        missing.push(line.clone());
    }

    if !missing.is_empty() {
        insert_missing_at_position(doc, &missing, &all_page_lines);
    }
}

fn append_footnote_text(doc: &Document, output: &mut String) {
    for footnote in &doc.footnotes {
        for inline in &footnote.content {
            if let InlineElement::Text(run) = inline {
                output.push_str(&run.text);
            }
        }
    }
}

fn line_matches_existing_text(
    line: &FrameLine,
    doc: &Document,
    full_doc_text: &str,
    full_doc_text_nospace: &str,
    heading_texts_nospace: &[String],
    exclude_text: &str,
) -> bool {
    let normalized = strip_cjk_spaces_str(&line.text);
    let demath = strip_math_italic(&line.text);
    let stripped = strip_visual_markers(&line.text);
    let demath_nospace = demath.replace(' ', "");
    let short = line.text.chars().count() < SHORT_LINE_CHAR_BOUNDARY;
    let short_exact = short
        && doc.body.elements.iter().any(|element| {
            if let BlockElement::Paragraph(paragraph) = element {
                paragraph_matches_short_line(paragraph, line.text.trim())
            } else {
                false
            }
        });

    short_exact
        || full_doc_text.contains(&line.text)
        || (!short
            && (full_doc_text.contains(&normalized)
                || full_doc_text.contains(&stripped)
                || full_doc_text.contains(&demath)
                || full_doc_text_nospace.contains(&demath_nospace)))
        || line_matches_emitted_heading(&line.text, heading_texts_nospace)
        || exclude_text.contains(&line.text)
}

fn line_words_are_already_emitted(
    line: &FrameLine,
    full_doc_text: &str,
    full_doc_text_nospace: &str,
) -> bool {
    let words: Vec<&str> = line.text.split_whitespace().collect();
    if words.len() >= MIN_LINE_WORDS {
        let significant = words
            .iter()
            .filter(|word| word.chars().count() >= MIN_SIGNIFICANT_WORD_CHARS)
            .count();
        let matched = words
            .iter()
            .filter(|word| {
                if word.chars().count() < MIN_SIGNIFICANT_WORD_CHARS {
                    return false;
                }
                full_doc_text.contains(**word) || {
                    let demath = strip_math_italic(word);
                    full_doc_text.contains(&demath)
                        || full_doc_text_nospace.contains(&demath.replace(' ', ""))
                }
            })
            .count();
        if significant >= MIN_SIGNIFICANT_WORDS && matched * MAJORITY_SCALE > significant {
            return true;
        }
    }

    words.iter().any(|word| {
        let cjk_count = word
            .chars()
            .filter(|c| matches!(*c, '\u{4E00}'..='\u{9FFF}'))
            .count();
        cjk_count >= MIN_CJK_WORD_CHARS && full_doc_text.contains(*word)
    })
}

fn line_cjk_is_already_emitted(line: &FrameLine, full_doc_text: &str, doc_cjk: &str) -> bool {
    let long_fragments = extract_cjk_fragments(&line.text, MIN_LONG_CJK_FRAGMENT_CHARS);
    if long_fragments
        .iter()
        .any(|fragment| full_doc_text.contains(fragment))
    {
        return true;
    }

    let short_fragments = extract_cjk_fragments(&line.text, MIN_SHORT_CJK_FRAGMENT_CHARS);
    if short_fragments.len() >= MIN_SHORT_CJK_FRAGMENTS {
        let matched = short_fragments
            .iter()
            .filter(|fragment| full_doc_text.contains(*fragment))
            .count();
        if matched * MAJORITY_SCALE > short_fragments.len() {
            return true;
        }
    }

    let without_citations = strip_citation_markers(&line.text);
    if without_citations.trim().is_empty() {
        return true;
    }
    let line_cjk: String = without_citations
        .chars()
        .filter(|c| is_cjk_ideograph(*c))
        .collect();
    let cjk_len = line_cjk.chars().count();
    let has_math = line.text.chars().any(|c| {
        ('\u{1D400}'..='\u{1D7FF}').contains(&c) || ('\u{2200}'..='\u{22FF}').contains(&c)
    });
    (cjk_len >= MIN_CJK_PROJECTION_CHARS || (cjk_len >= MIN_MATH_CJK_PROJECTION_CHARS && has_math))
        && doc_cjk.contains(&line_cjk)
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

pub(super) fn extract_lines_from_all_pages(
    paged: &PagedDocument,
    margins: super::page::MarginsPt,
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
                    .entry(super::page::pt_to_tenths(item.size_pt))
                    .or_default() += item.text.len();
            }
            // Tie-break on the smaller size so the detected body size is
            // deterministic (a bare `max_by_key` would pick whichever size the
            // HashMap iterated first when two tie on glyph count).
            super::stats::dominant_key(sizes.iter().map(|(size, count)| (size, *count)))
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
        let (body_top, body_bottom) = super::page::find_body_zone(
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
                    let half_pt = super::page::pt_to_half_pt(item.size_pt);
                    if half_pt != super::page::pt_to_half_pt(body_size) {
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
    super::frames::visit_frame_items(frame, true, &mut |position, item| {
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

fn count_title_lines(paged_lines: &[FrameLine], doc: &Document) -> usize {
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

/// Map Unicode Mathematical Italic/Bold/Script characters to ASCII equivalents.
/// Paged output renders math as Unicode math italic (U+1D400-U+1D7FF) while
/// OMML stores them as plain ASCII with formatting attributes.
fn strip_math_italic(text: &str) -> String {
    text.chars()
        .map(|c| {
            let cp = u32::from(c);
            match cp {
                // Math italic small a-z: U+1D44E-U+1D467
                0x1D44E..=0x1D467 => math_letter(c, cp, 0x1D44E, 'a'),
                // Math italic capital A-Z: U+1D434-U+1D44D
                0x1D434..=0x1D44D => math_letter(c, cp, 0x1D434, 'A'),
                // Math bold small a-z: U+1D41A-U+1D433
                0x1D41A..=0x1D433 => math_letter(c, cp, 0x1D41A, 'a'),
                // Math bold capital A-Z: U+1D400-U+1D419
                0x1D400..=0x1D419 => math_letter(c, cp, 0x1D400, 'A'),
                // Math bold italic small a-z: U+1D482-U+1D49B
                0x1D482..=0x1D49B => math_letter(c, cp, 0x1D482, 'a'),
                // Math bold italic capital A-Z: U+1D468-U+1D481
                0x1D468..=0x1D481 => math_letter(c, cp, 0x1D468, 'A'),
                // Math italic h: U+210E
                0x210E => 'h',
                _ => c,
            }
        })
        .collect()
}

fn math_letter(original: char, codepoint: u32, range_start: u32, ascii_start: char) -> char {
    char::from_u32(u32::from(ascii_start) + codepoint - range_start).unwrap_or(original)
}

/// Cancel every whitespace character, so a paged render and the emitted text of the
/// same heading compare equal despite the `format!("{numbers} ")` space (and any
/// layout spacing) between a heading's number and its title.
fn cancel_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// True when a paged line is a re-scraped emitted heading. The emitted heading
/// already carries Typst's computed number (from the semantic `HeadingElem`), so an
/// exact whitespace-cancelled match against the emitted heading texts dedups the line
/// for any numbering scheme or language — replacing the old hardcoded Chinese-numeral
/// prefix stripping. Exact (not substring) match keeps it from ever suppressing body
/// prose that merely contains a short heading's words.
fn line_matches_emitted_heading(line: &str, heading_texts_nospace: &[String]) -> bool {
    let line_nospace = cancel_whitespace(&strip_math_italic(line));
    !line_nospace.is_empty() && heading_texts_nospace.iter().any(|h| h == &line_nospace)
}

/// Whether `c` is a CJK ideograph (the ranges used for projection/fragments).
fn is_cjk_ideograph(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}

/// Exact-match a short paged line against a paragraph — as the whole paragraph
/// text, or as any of its forced-line-break segments. A paragraph `a \ b`
/// renders as two paged lines "a" and "b"; neither equals the concatenated
/// `text_content()`, so a whole-paragraph comparison alone re-injects each
/// segment as a centered orphan duplicate. Matching stays exact (never
/// substring) so a short placed line that merely occurs inside longer body
/// prose is still recovered.
fn paragraph_matches_short_line(p: &Paragraph, line: &str) -> bool {
    if p.text_content().trim() == line {
        return true;
    }
    let mut segment = String::new();
    for inline in &p.inlines {
        match inline {
            InlineElement::Text(run) => {
                if run.line_break {
                    if segment.trim() == line {
                        return true;
                    }
                    segment.clear();
                } else {
                    segment.push_str(&run.text);
                }
            }
            InlineElement::Hyperlink { runs, .. } => {
                for run in runs {
                    segment.push_str(&run.text);
                }
            }
            _ => {}
        }
    }
    segment.trim() == line
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

/// Whitespace/math-stripped text of every cell in every real table in the body.
/// Used to recognize a recovered multi-column line as a re-scraped table row.
fn collect_table_cell_texts(doc: &Document) -> Vec<String> {
    let mut cells = Vec::new();
    for elem in &doc.body.elements {
        if let BlockElement::Table(t) = elem {
            collect_cells_from_table(t, &mut cells);
        }
    }
    cells
}

fn collect_cells_from_table(table: &Table, out: &mut Vec<String>) {
    for row in &table.rows {
        for cell in &row.cells {
            if cell.content.is_empty() {
                for para in &cell.paragraphs {
                    push_cell_text(para, out);
                }
            } else {
                for content in &cell.content {
                    match content {
                        CellContent::Paragraph(p) => push_cell_text(p, out),
                        CellContent::Table(nested) => collect_cells_from_table(nested, out),
                    }
                }
            }
        }
    }
}

fn push_cell_text(para: &Paragraph, out: &mut Vec<String>) {
    let text = cancel_whitespace(&strip_math_italic(&para.full_text_content()));
    if text.chars().count() >= MIN_TABLE_CELL_CHARS {
        out.push(text);
    }
}

/// Whether a recovered multi-column line's columns are each (a substring of) some
/// real table cell — a majority match — i.e. the line is a re-scraped table row.
/// `cell.contains(col)` tolerates a column that wrapped (the recovered column is
/// then a prefix/suffix of the full cell text).
fn line_matches_table_cells(line: &FrameLine, cells: &[String]) -> bool {
    let cols: Vec<String> = line
        .x_clusters
        .iter()
        .map(|c| {
            let joined: String = c.runs.iter().map(|r| r.text.as_str()).collect();
            cancel_whitespace(&strip_math_italic(&joined))
        })
        .filter(|t| t.chars().count() >= MIN_TABLE_COLUMN_CHARS)
        .collect();
    if cols.is_empty() {
        return false;
    }
    let matched = cols
        .iter()
        .filter(|col| cells.iter().any(|cell| cell.contains(col.as_str())))
        .count();
    matched * MAJORITY_SCALE >= cols.len()
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

        let has_large_gap = line_has_large_cluster_gap(line);
        let is_real_grid = line.x_clusters.len() >= MIN_COLUMN_CLUSTERS
            && has_large_gap
            && line.x_clusters.iter().all(|c| {
                c.runs.iter().map(|r| r.text.chars().count()).sum::<usize>()
                    >= MIN_RECOVERED_GRID_COLUMN_CHARS
            });
        if is_real_grid {
            let last_cluster = &line.x_clusters[line.x_clusters.len() - 1];
            let tab_pos = super::page::pt_to_twips(last_cluster.x_pt);
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
        .map(|run| {
            f64::from(
                run.size_half_pt
                    .unwrap_or(DEFAULT_RECOVERED_RUN_SIZE_HALF_PT),
            ) / 2.0
        })
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
        .map(|run| {
            f64::from(
                run.size_half_pt
                    .unwrap_or(DEFAULT_RECOVERED_RUN_SIZE_HALF_PT),
            ) / 2.0
        })
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
    if total_elements == 0 || paged.pages().is_empty() {
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

    // `Introspector::page` now returns `Option<NonZeroUsize>` (0.14 returned the
    // `NonZeroUsize` directly); a location without a known page falls back to 1.
    let tag_pages: Vec<usize> = locs
        .iter()
        .map(|loc| paged.introspector().page(*loc).map_or(1, NonZeroUsize::get))
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

/// Whether the source AST contains a `#line(...)` call — the only construct that
/// should become a body horizontal rule. `#line()` has no HTML element and is
/// consumed during layout, so it is recovered from geometry; but a table's
/// border rules and a footnote separator are *also* wide horizontal lines in the
/// geometry. Gating rule recovery on a real `line()` call stops those from being
/// invented as body rules (the same source-AST-authority rule as colbreak).
pub(super) fn source_declares_line_rule(source: &str) -> bool {
    fn has_line_call(node: &typst_syntax::SyntaxNode) -> bool {
        if node.kind() == typst_syntax::SyntaxKind::FuncCall
            && node
                .cast::<typst_syntax::ast::FuncCall<'_>>()
                .is_some_and(|fc| {
                    matches!(fc.callee(), typst_syntax::ast::Expr::Ident(i) if i.as_str() == "line")
                })
        {
            return true;
        }
        node.children().any(has_line_call)
    }
    has_line_call(&typst_syntax::parse(source))
}

/// Detect horizontal line shapes and insert horizontal rule paragraphs.
pub(super) fn insert_horizontal_rules_from_paged(
    paged: &PagedDocument,
    doc: &mut Document,
    element_page_map: &[usize],
    sources: &[String],
) {
    // Only recover rules the source actually declares with `#line()`; without one,
    // a wide line in the geometry is a table border or a footnote separator. The
    // declaration may live in ANY reachable source file — a template imported by
    // the main file draws its separators just as authoritatively.
    if !sources.iter().any(|s| source_declares_line_rule(s)) {
        return;
    }
    let total_pages = paged.pages().len();
    if total_pages == 0 {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    let mut hrules: Vec<(usize, f64)> = Vec::new();

    for (page_idx, page) in paged.pages().iter().enumerate() {
        let page_width = page.frame.width().to_pt();
        let content_width = page_width * HORIZONTAL_RULE_PAGE_WIDTH_RATIO;

        let mut lines = Vec::new();
        collect_horizontal_lines(&page.frame, content_width, &mut lines);

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
            super::stats::proportional_index_rounded(
                page_num.saturating_sub(1),
                total_pages,
                total_elements,
            )
            .min(total_elements)
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

        if full_doc_text.len() < MIN_RULE_DOCUMENT_TEXT_BYTES {
            continue;
        }

        let mut hr_para = Paragraph::new();
        hr_para.horizontal_rule = true;
        doc.body
            .elements
            .insert(insert_idx, BlockElement::Paragraph(hr_para));
    }
}

fn collect_horizontal_lines(frame: &Frame, min_width: f64, lines: &mut Vec<f64>) {
    super::frames::visit_frame_items(frame, true, &mut |position, item| {
        if let FrameItem::Shape(shape, _) = item
            && let typst::visualize::Geometry::Line(end_pt) = &shape.geometry
        {
            let line_width = end_pt.x.to_pt().abs();
            let line_height = end_pt.y.to_pt().abs();
            if line_width >= min_width && line_height < MAX_HORIZONTAL_RULE_HEIGHT_PT {
                lines.push(position.y.to_pt());
            }
        }
    });
}

/// Merge consecutive paragraphs whose text appears on the same visual line in
/// the paged output.  This fixes cases where Typst's HTML export splits inline
/// content (e.g. `super()` calls interleaved with author names in a `#for` loop)
/// into separate block-level elements that become separate Word paragraphs.
pub(super) fn merge_same_line_paragraphs(doc: &mut Document) {
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

/// Per-table rule evidence harvested from the paged frames: horizontal rule
/// thicknesses (eighths of a point) and whether any cell-height vertical line
/// is drawn inside the table.
#[derive(Default)]
struct TableRules {
    sizes: Vec<u32>,
    has_vertical: bool,
}

/// Style each top-level table from the rules Typst actually drew FOR THAT
/// TABLE. The paged frames carry the introspection `Tag::Start`/`Tag::End`
/// brackets of every `TableElem`, so rule shapes are attributed to the table
/// whose bracket is open where they are painted — a footnote separator or an
/// author `#line()` (outside any bracket) can no longer restyle tables, and a
/// boxed table elsewhere no longer disables a genuine three-line table.
///
/// Per table: vertical lines → boxed (leave `borders` unset; the writer draws
/// a uniform grid), horizontal rules only → three-line (thick top/bottom,
/// thin header separator), no rules at all → the author drew the table
/// borderless (`stroke: none`); emit explicit nil borders so the writer's
/// uniform-grid fallback doesn't invent a box.
pub(super) fn detect_three_line_tables(paged: &PagedDocument, doc: &mut Document) {
    let body_table_count = doc
        .body
        .elements
        .iter()
        .filter(|e| matches!(e, BlockElement::Table(_)))
        .count();
    if body_table_count == 0 {
        return;
    }

    let mut stack: Vec<Location> = Vec::new();
    let mut order: Vec<Location> = Vec::new();
    let mut per_table: HashMap<Location, TableRules> = HashMap::new();
    for page in paged.pages() {
        collect_table_rules(&page.frame, &mut stack, &mut order, &mut per_table);
    }

    // Document order of top-level paged tables must line up with the body's
    // table order; when it doesn't (a table the HTML walk dropped, or vice
    // versa), attribution would be misaligned — degrade to the writer's
    // uniform fallback rather than stamp the wrong table.
    if order.len() != body_table_count {
        return;
    }

    let mut locs = order.iter();
    for el in &mut doc.body.elements {
        let BlockElement::Table(t) = el else { continue };
        let Some(rules) = locs.next().and_then(|loc| per_table.get(loc)) else {
            continue;
        };
        if rules.has_vertical {
            // Boxed grid: keep `borders` unset — the writer's uniform fallback.
            continue;
        }
        if rules.sizes.is_empty() {
            // No rules drawn: `stroke: none`. Explicit nil borders on every side.
            t.borders = Some(TableBorders {
                top: None,
                bottom: None,
                left: None,
                right: None,
                inside_h: None,
                inside_v: None,
                header_sep: None,
                header_rows: 0,
            });
            continue;
        }
        let thin = *rules.sizes.iter().min().expect("non-empty");
        let thick = *rules.sizes.iter().max().expect("non-empty");
        t.borders = Some(TableBorders {
            top: Some(thick),
            bottom: Some(thick),
            left: None,
            right: None,
            inside_h: None,
            inside_v: None,
            header_sep: Some(thin),
            header_rows: 1,
        });
    }
}

/// Depth-first, in-paint-order walk attributing rule shapes to the innermost
/// open `TableElem` tag bracket. Only top-level brackets are recorded in
/// `order`/`per_table`; rules inside nested tables still count toward the
/// outer table's evidence (they ARE lines drawn within its region), and rules
/// outside every bracket are ignored entirely.
///
/// NB (evaluated 2026-07-12, typst 0.15): `Selector::within` cannot replace this
/// stack — it scopes introspector queries over *locatable elements*, while the
/// rule strokes attributed here are plain `FrameItem::Shape`s in the paged
/// frames, invisible to the introspector. Frame-order bracket matching is the
/// only source that pairs a drawn line with its table.
fn collect_table_rules(
    frame: &Frame,
    stack: &mut Vec<Location>,
    order: &mut Vec<Location>,
    per_table: &mut HashMap<Location, TableRules>,
) {
    use typst::introspection::Tag;
    super::frames::visit_frame_items(frame, false, &mut |_, item| {
        match item {
            FrameItem::Tag(Tag::Start(content, _)) => {
                if content.elem() == typst_library::model::TableElem::ELEM
                    && let Some(loc) = content.location()
                {
                    if stack.is_empty() {
                        order.push(loc);
                        per_table.entry(loc).or_default();
                    }
                    stack.push(loc);
                }
            }
            FrameItem::Tag(Tag::End(loc, ..)) => {
                if stack.last() == Some(loc) {
                    stack.pop();
                }
            }
            FrameItem::Shape(shape, _) => {
                let Some(&owner) = stack.first() else {
                    return; // not inside any table — footnote separator, #line(), …
                };
                if let typst::visualize::Geometry::Line(end) = &shape.geometry {
                    let dx = end.x.to_pt().abs();
                    let dy = end.y.to_pt().abs();
                    let thickness_pt = shape.stroke.as_ref().map_or(0.0, |s| s.thickness.to_pt());
                    if thickness_pt <= 0.0 {
                        return;
                    }
                    let sz =
                        super::page::pt_to_eighth_pt(thickness_pt).max(MIN_TABLE_RULE_EIGHTH_PT);
                    let rules = per_table.entry(owner).or_default();
                    if dy < TABLE_RULE_AXIS_TOLERANCE_PT
                        && dx >= MIN_HORIZONTAL_TABLE_RULE_LENGTH_PT
                    {
                        // A wide, flat rule — a horizontal table line.
                        rules.sizes.push(sz);
                    } else if dx < TABLE_RULE_AXIS_TOLERANCE_PT
                        && dy >= MIN_VERTICAL_TABLE_RULE_LENGTH_PT
                    {
                        // A vertical line tall enough to be a cell border → boxed grid.
                        rules.has_vertical = true;
                    }
                }
            }
            _ => {}
        }
    });
}
