//! Bibliography and citation tests.

use crate::common::{fixture_doc_xml, paragraph_containing};
use std::io::Cursor;
use std::path::Path;

#[test]
fn bibliography_citations_are_markers_not_field_refs() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Citations must NOT be REF fields to bibliography keys: a REF to a
    // non-existent bookmark renders in Word as "Error! Reference source not
    // found". They render instead as the marker Typst produced.
    for key in ["REF smith2020", "REF knuth1997", "REF wang2023"] {
        assert!(
            !xml.contains(key),
            "citation must not be a REF field: {key}"
        );
    }
    // The default style renders inline numeric markers ([1]/[2]/[3]).
    assert!(
        xml.contains("[1]") && xml.contains("[2]") && xml.contains("[3]"),
        "expected inline numeric citation markers: {xml}"
    );
}

#[test]
fn superscript_citation_style_raises_the_marker() {
    let xml = fixture_doc_xml("bibliography_superscript");
    // A superscript numeric style (here "nature") must raise the in-text marker,
    // detected from the rendered <sup> — not assumed from the style name.
    assert!(
        !xml.contains("REF smith2020"),
        "citation must not be a broken REF field: {xml}"
    );
    assert!(
        xml.contains(r#"<w:vertAlign w:val="superscript"/>"#),
        "superscript citation style must produce a raised marker: {xml}"
    );
}

#[test]
fn bibliography_entries_are_not_a_bulleted_list() {
    // Regression: bibliography entries arrive as a Typst <ul>, so each kept a
    // bullet-list numPr on top of its "[n]" label — a double marker. The "[n]" is
    // the marker; entries should carry only a hanging indent, no list numbering.
    let doc_xml = fixture_doc_xml("bibliography_basic");
    let entry = paragraph_containing(&doc_xml, "An Example Article");
    assert!(
        !entry.contains("<w:numPr>"),
        "bibliography entries must not be a bulleted/numbered list:\n{entry}"
    );
    assert!(
        entry.contains("w:hanging"),
        "bibliography entries should keep a hanging indent"
    );
}

#[test]
fn bibliography_style_is_defined() {
    // The reference field-code paragraph carries the "Bibliography" style; it must
    // be defined in styles.xml, not left as a dangling reference relying on Word's
    // built-in (which WPS/LibreOffice may not have).
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/bibliography_basic.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let styles = std::io::read_to_string(reader.by_name("word/styles.xml").unwrap()).unwrap();
    assert!(
        styles.contains(r#"w:styleId="Bibliography""#),
        "styles.xml should define the Bibliography style"
    );
}

#[test]
fn bibliography_produces_bibliography_sdt() {
    let xml = fixture_doc_xml("bibliography_basic");
    assert!(
        xml.contains("w:bibliography"),
        "expected w:bibliography SDT marker"
    );
    assert!(
        xml.contains("BIBLIOGRAPHY"),
        "expected BIBLIOGRAPHY field code"
    );
}

#[test]
fn bibliography_has_custom_xml_sources() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    assert!(
        archive.by_name("customXml/item1.xml").is_ok(),
        "expected customXml/item1.xml in ZIP"
    );
    let xml = std::io::read_to_string(archive.by_name("customXml/item1.xml").unwrap()).unwrap();
    assert!(xml.contains("b:Sources"), "expected b:Sources root");
    assert!(xml.contains("smith2020"), "expected smith2020 tag");
    assert!(xml.contains("knuth1997"), "expected knuth1997 tag");
    assert!(xml.contains("wang2023"), "expected wang2023 tag");
    assert!(
        xml.contains("JournalArticle"),
        "expected JournalArticle source type"
    );
    assert!(
        xml.contains("Book"),
        "expected Book source type for knuth1997"
    );
}

#[test]
fn bibliography_custom_xml_has_author_metadata() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let xml = std::io::read_to_string(archive.by_name("customXml/item1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("<b:Last>Smith</b:Last>"),
        "expected Smith author"
    );
    assert!(xml.contains("<b:Year>2020</b:Year>"), "expected year 2020");
    assert!(xml.contains("<b:Title>"), "expected title element");
    assert!(
        xml.contains("<b:Last>Knuth</b:Last>"),
        "expected Knuth author"
    );
    assert!(xml.contains("<b:Year>1997</b:Year>"), "expected year 1997");
}

#[test]
fn bibliography_citation_sources_count() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    assert_eq!(
        doc.citation_sources.len(),
        3,
        "expected 3 citation sources, got {}",
        doc.citation_sources.len()
    );
}

#[test]
fn bibliography_field_codes_have_begin_and_end() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Both inline citations and bibliography block use field codes
    assert!(
        xml.contains("fldCharType=\"begin\""),
        "expected fldChar begin in citation field"
    );
    assert!(
        xml.contains("fldCharType=\"end\""),
        "expected fldChar end in citation field"
    );
}

#[test]
fn bibliography_display_text_present() {
    let xml = fixture_doc_xml("bibliography_basic");
    // The display text for citations should appear (e.g., "[1]", "[2]", "[3]")
    assert!(xml.contains("[1]"), "expected display text [1]");
    assert!(xml.contains("[2]"), "expected display text [2]");
    assert!(xml.contains("[3]"), "expected display text [3]");
}

#[test]
fn bibliography_section_heading_present() {
    let xml = fixture_doc_xml("bibliography_basic");
    // The bibliography section heading should be present
    assert!(
        xml.contains("Bibliography"),
        "expected Bibliography heading text"
    );
}

#[test]
fn bibliography_body_text_preserved() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Body text around citations should be preserved
    assert!(
        xml.contains("Introduction"),
        "expected Introduction heading"
    );
    assert!(xml.contains("Methods"), "expected Methods heading");
    assert!(
        xml.contains("methodology is sound"),
        "expected body text near citation"
    );
}

#[test]
fn multiple_bibliographies_collect_sources_from_all() {
    let path = "../../tests/fixtures/bibliography_multiple.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let tags: Vec<&str> = doc
        .citation_sources
        .iter()
        .map(|s| s.tag.as_str())
        .collect();
    assert!(
        tags.contains(&"alpha2021"),
        "first bib's key missing: {tags:?}"
    );
    assert!(
        tags.contains(&"beta2022"),
        "second bib's key missing: {tags:?}"
    );
}

#[test]
fn multiple_bibliographies_render_two_blocks() {
    let doc_xml = fixture_doc_xml("bibliography_multiple");
    assert!(
        doc_xml.contains("First Library Article"),
        "first bib entry text missing"
    );
    assert!(
        doc_xml.contains("Second Library Book"),
        "second bib entry text missing"
    );
    let sdt_count = doc_xml.matches("<w:bibliography/>").count();
    assert_eq!(
        sdt_count, 2,
        "expected one bibliography SDT per #bibliography()"
    );
}

#[test]
fn duplicate_citation_key_across_bibliographies_keeps_first_occurrence() {
    // Regression: typst allows the same citation key to appear in separate
    // `#bibliography()` calls in one document (it compiles without error), but
    // the merge that combines each bibliography's hayagriva::Library used
    // `merge_library`, which is last-wins (`Library::push` -> `IndexMap::insert`
    // overwrites). That silently let a later bibliography's entry clobber an
    // earlier one's metadata for the same key. The fix keeps the first
    // (earliest-in-document) occurrence via `merge_library_keep_first`. See
    // tests/fixtures/bibliography_duplicate_key.typ, which cites `dup2020` in
    // both "Part One" (bibliography A, title "First Title") and "Part Two"
    // (bibliography B, title "Second Title").
    let path = "../../tests/fixtures/bibliography_duplicate_key.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let dup_sources: Vec<_> = doc
        .citation_sources
        .iter()
        .filter(|s| s.tag == "dup2020")
        .collect();
    assert_eq!(
        dup_sources.len(),
        1,
        "BibliographyElem::keys() yields one (label, ..) tuple per bibliography \
         that defines the key, so the naive filter_map produced one CitationSource \
         PER bibliography (same tag, same writer-derived GUID) instead of deduping \
         by tag; expected exactly one citation_sources entry for dup2020, got: \
         {dup_sources:?}"
    );
    for src in &dup_sources {
        assert_eq!(
            src.title.as_deref(),
            Some("First Title"),
            "duplicate key across bibliographies must keep the FIRST bibliography's \
             metadata (document order), not the last one's"
        );
    }
}

#[test]
fn citations_link_to_their_bibliography_entry() {
    // @key citations must become clickable cross-references to the matching
    // reference entry: each bib entry gets a bookmark, each in-text citation an
    // internal hyperlink (w:anchor) pointing at it. Typst's HTML already carries the
    // pairing (citation href="#loc-N" <-> entry id="loc-N"). See
    // tests/fixtures/bibliography_basic.typ.
    let doc_xml = fixture_doc_xml("bibliography_basic");
    let vals = |pre: &str| -> Vec<String> {
        doc_xml
            .match_indices(pre)
            .filter_map(|(i, _)| {
                doc_xml[i + pre.len()..]
                    .split('"')
                    .next()
                    .map(str::to_string)
            })
            .collect()
    };
    let anchors = vals(r#"<w:hyperlink w:anchor=""#);
    let bookmarks: std::collections::HashSet<String> = vals(r#"w:name=""#).into_iter().collect();
    assert!(
        !anchors.is_empty(),
        "@key citations must become internal hyperlinks:\n{doc_xml}"
    );
    for a in &anchors {
        assert!(
            bookmarks.contains(a),
            "citation anchor {a:?} has no matching bibliography bookmark; bookmarks={bookmarks:?}"
        );
    }
}
