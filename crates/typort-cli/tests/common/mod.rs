//! Shared fixture-conversion and XML-scanning helpers for the area modules in
//! the `integration` test binary. This file does not itself match Cargo's test
//! entry-point patterns; `tests/integration/main.rs` pulls it in explicitly.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// The path to `tests/fixtures/<fixture>.typ`.
pub fn fixture_path(fixture: &str) -> PathBuf {
    PathBuf::from(format!("../../tests/fixtures/{fixture}.typ"))
}

/// Convert `tests/fixtures/<fixture>.typ` and return the public document model.
pub fn fixture_document(fixture: &str) -> typort_ooxml::Document {
    fixture_document_with_font_dirs(fixture, &[])
}

/// Convert a fixture and return both its world and public document model.
pub fn fixture_document_with_world(
    fixture: &str,
) -> (typort_core::TyportWorld, typort_ooxml::Document) {
    let world = typort_core::TyportWorld::new(&fixture_path(fixture)).unwrap();
    let document = typort_core::convert::convert(&world).unwrap();
    (world, document)
}

/// Convert `tests/fixtures/<fixture>.typ` with extra font directories loaded and
/// return the public document model.
pub fn fixture_document_with_font_dirs(
    fixture: &str,
    font_dirs: &[PathBuf],
) -> typort_ooxml::Document {
    let world =
        typort_core::TyportWorld::with_font_dirs(&fixture_path(fixture), font_dirs).unwrap();
    typort_core::convert::convert(&world).unwrap()
}

/// The parts produced by one fixture conversion.
pub struct FixturePackage {
    parts: Vec<(String, Vec<u8>)>,
    byte_len: usize,
}

impl FixturePackage {
    /// Return the serialized package size in bytes.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    /// Return every part name in package order.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().map(|(name, _)| name.as_str())
    }

    /// Return a package part as text.
    pub fn part_text(&self, name: &str) -> &str {
        let bytes = &self
            .parts
            .iter()
            .find(|(part_name, _)| part_name == name)
            .unwrap_or_else(|| panic!("package should contain {name:?}"))
            .1;
        std::str::from_utf8(bytes)
            .unwrap_or_else(|error| panic!("{name:?} should be text: {error}"))
    }
}

/// Convert `tests/fixtures/<fixture>.typ` and return all package parts.
pub fn fixture_package(fixture: &str) -> FixturePackage {
    fixture_package_from_document(&fixture_document(fixture))
}

/// Package a converted fixture document and return all parts.
pub fn fixture_package_from_document(doc: &typort_ooxml::Document) -> FixturePackage {
    let mut buf = Vec::new();
    typort_ooxml::write_docx(doc, Cursor::new(&mut buf)).unwrap();

    let byte_len = buf.len();
    let mut archive = zip::ZipArchive::new(Cursor::new(buf)).unwrap();
    let mut parts = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let mut part = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes).unwrap();
        parts.push((part.name().to_owned(), bytes));
    }
    FixturePackage { parts, byte_len }
}

/// Convert `tests/fixtures/<fixture>.typ` and return the named docx part.
pub fn fixture_part(fixture: &str, part: &str) -> String {
    fixture_package(fixture).part_text(part).to_owned()
}

/// Convert `tests/fixtures/<fixture>.typ` with extra font directories loaded
/// (e.g. `tests/fonts` for a variable-font fixture) and return the named docx
/// part. Both this and `fixture_part` build on `fixture_document*` and
/// `fixture_package_from_document`, so there is one conversion pipeline.
pub fn fixture_part_with_font_dirs(fixture: &str, part: &str, font_dirs: &[PathBuf]) -> String {
    fixture_package_from_document(&fixture_document_with_font_dirs(fixture, font_dirs))
        .part_text(part)
        .to_owned()
}

/// The directory containing a fixture-relative path.
pub fn fixture_dir(path: impl AsRef<Path>) -> PathBuf {
    PathBuf::from("../../tests/fixtures").join(path)
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
