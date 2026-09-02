//! Deduplication corpus and predicates for recovered lines.

use typort_ooxml::document::{
    BlockElement, CellContent, Document, InlineElement, Paragraph, ParagraphStyle, Table,
};

use super::super::text_norm::{
    cancel_whitespace, extract_cjk_fragments, is_cjk_ideograph, strip_citation_markers,
    strip_cjk_spaces_str, strip_math_italic, strip_visual_markers,
};
use super::lines::{FrameLine, MIN_COLUMN_CLUSTERS};

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
/// Minimum normalized table-cell text retained for matching.
const MIN_TABLE_CELL_CHARS: usize = 2;
/// Minimum recovered table-column text retained for matching.
const MIN_TABLE_COLUMN_CHARS: usize = 3;
/// Multiplier used to express strict or inclusive majority comparisons.
const MAJORITY_SCALE: usize = 2;

pub(super) struct DeduplicationCorpus {
    full_doc_text: String,
    full_doc_text_nospace: String,
    doc_cjk: String,
    table_cell_texts: Vec<String>,
    heading_texts_nospace: Vec<String>,
    exclude_text: String,
}

impl DeduplicationCorpus {
    pub(super) fn from_document(doc: &Document) -> Self {
        let mut full_doc_text = extract_doc_text(doc);
        // Footnote bodies are real footnotes in the model (in footnotes.xml), not body
        // content. Fold their text into the dedup corpus so the page-bottom footnote
        // zone is never re-scraped into orphan body paragraphs. (They are also kept in
        // the exclude corpus below for the exact-line path.)
        append_footnote_text(doc, &mut full_doc_text);
        let full_doc_text_nospace = strip_math_italic(&full_doc_text).replace(' ', "");
        // CJK-only projection recognizes paged prose broken up by interleaved OMML
        // math, superscript citations, or heading numbers.
        let doc_cjk = full_doc_text
            .chars()
            .filter(|c| is_cjk_ideograph(*c))
            .collect();
        let table_cell_texts = collect_table_cell_texts(doc);
        let heading_texts_nospace = doc
            .body
            .elements
            .iter()
            .filter_map(|e| match e {
                BlockElement::Paragraph(p)
                    if matches!(p.style, Some(ParagraphStyle::Heading(_))) =>
                {
                    let text = cancel_whitespace(&strip_math_italic(&p.text_content()));
                    (!text.is_empty()).then_some(text)
                }
                _ => None,
            })
            .collect();
        let mut exclude_text = extract_header_footer_text(doc);
        append_footnote_text(doc, &mut exclude_text);

        Self {
            full_doc_text,
            full_doc_text_nospace,
            doc_cjk,
            table_cell_texts,
            heading_texts_nospace,
            exclude_text,
        }
    }

    pub(super) fn contains(&self, line: &FrameLine, doc: &Document) -> bool {
        line_matches_existing_text(
            line,
            doc,
            &self.full_doc_text,
            &self.full_doc_text_nospace,
            &self.heading_texts_nospace,
            &self.exclude_text,
        ) || line_words_are_already_emitted(line, &self.full_doc_text, &self.full_doc_text_nospace)
            || line_cjk_is_already_emitted(line, &self.full_doc_text, &self.doc_cjk)
            || (!self.table_cell_texts.is_empty()
                && line.x_clusters.len() >= MIN_COLUMN_CLUSTERS
                && line_matches_table_cells(line, &self.table_cell_texts))
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
