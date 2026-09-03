//! Recovery of layout-only content omitted by Typst's HTML export.

mod deduplication;
mod horizontal_rules;
mod insertion;
mod lines;
mod table_rules;

pub(super) use horizontal_rules::{build_element_page_map, insert_horizontal_rules_from_paged};
pub(super) use table_rules::detect_three_line_tables;

use deduplication::DeduplicationCorpus;
use insertion::insert_missing_at_position;
use lines::{count_title_lines, extract_lines_from_all_pages};
use typort_ooxml::document::Document;
use typst_layout::PagedDocument;

/// Minimum rendered line length eligible for recovery.
const MIN_RECOVERED_LINE_CHARS: usize = 2;

pub(super) fn recover_missing_content(paged: &PagedDocument, doc: &mut Document) {
    let margins = super::page::MarginsPt::from_settings(&doc.page_settings);
    let all_page_lines = extract_lines_from_all_pages(paged, margins);
    if all_page_lines.is_empty() {
        return;
    }

    let title_line_count = count_title_lines(&all_page_lines, doc);
    let corpus = DeduplicationCorpus::from_document(doc);

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
        if corpus.contains(line, doc) {
            continue;
        }
        missing.push(line.clone());
    }

    if !missing.is_empty() {
        insert_missing_at_position(doc, &missing, &all_page_lines);
    }
}
