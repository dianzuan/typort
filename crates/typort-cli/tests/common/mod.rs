//! Shared fixture-conversion and XML-scanning helpers for the `integration` and
//! `review_regressions` test binaries. Cargo compiles each `tests/*.rs` (and
//! `tests/*/main.rs`) as its own binary, so this file — which does not itself
//! match either pattern — is not a test binary; it is pulled in via `mod
//! common;` (for a flat file, resolving to this `tests/common/mod.rs`) or
//! `#[path = "../common/mod.rs"] mod common;` (from `tests/integration/main.rs`).
//!
//! Not every consuming binary/module uses every helper here, which would
//! otherwise warn under `dead_code` from that binary's point of view.
#![allow(dead_code)]

use std::io::Cursor;
use std::path::PathBuf;

/// Convert `tests/fixtures/<fixture>.typ` and return the named docx part.
pub fn fixture_part(fixture: &str, part: &str) -> String {
    let path = PathBuf::from(format!("../../tests/fixtures/{fixture}.typ"));
    let world = typort_core::TyportWorld::new(&path).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name(part).unwrap()).unwrap()
}

/// Convert `tests/fixtures/<fixture>.typ` and return `word/document.xml`.
pub fn fixture_doc_xml(fixture: &str) -> String {
    fixture_part(fixture, "word/document.xml")
}

/// Convert `tests/fixtures/<fixture>.typ` and return `word/styles.xml`.
pub fn fixture_styles_xml(fixture: &str) -> String {
    fixture_part(fixture, "word/styles.xml")
}

/// Return the single `<w:p>...</w:p>` block that contains `needle`.
pub fn paragraph_containing<'a>(doc_xml: &'a str, needle: &str) -> &'a str {
    let pos = doc_xml
        .find(needle)
        .unwrap_or_else(|| panic!("document should contain {needle:?}"));
    let start = doc_xml[..pos]
        .rfind("<w:p>")
        .or_else(|| doc_xml[..pos].rfind("<w:p "))
        .expect("paragraph start");
    let end = doc_xml[pos..]
        .find("</w:p>")
        .map(|e| pos + e)
        .expect("paragraph end");
    &doc_xml[start..end]
}

/// Concatenated `<w:t>` text of every `<w:p>` in document order.
/// Matches both `<w:p>` and `<w:p ...>` (attribute-bearing) paragraphs.
pub fn paragraph_texts(doc_xml: &str) -> Vec<String> {
    doc_xml
        .match_indices("<w:p")
        .filter(|(i, _)| matches!(doc_xml.as_bytes().get(i + 4), Some(b'>' | b' ')))
        .map(|(start, _)| {
            let end = doc_xml[start..]
                .find("</w:p>")
                .map_or(doc_xml.len(), |e| start + e);
            let block = &doc_xml[start..end];
            let mut t = String::new();
            let mut rest = block;
            while let Some(o) = rest.find("<w:t") {
                let after = &rest[o..];
                if let Some(gt) = after.find('>') {
                    let content = &after[gt + 1..];
                    if let Some(close) = content.find("</w:t>") {
                        t.push_str(&content[..close]);
                        rest = &content[close..];
                        continue;
                    }
                }
                break;
            }
            t
        })
        .collect()
}
