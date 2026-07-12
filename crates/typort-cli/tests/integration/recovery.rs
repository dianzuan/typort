//! Geometry-recovery heuristic tests (see convert/recovery.rs and convert/page.rs).

use crate::common;
use crate::common::fixture_doc_xml;
use std::io::Cursor;
use std::path::Path;

#[test]
fn long_left_heading_not_misclassified_as_centered() {
    // Regression: a long left-aligned heading whose text spans most of the line
    // has a text-center near the page center and was wrongly marked centered. See
    // tests/fixtures/edge_long_left_heading.typ.
    let doc_xml = fixture_doc_xml("edge_long_left_heading");
    assert!(
        !doc_xml.contains(r#"<w:jc w:val="center"/>"#),
        "long left-aligned headings must not be misclassified as centered:\n{doc_xml}"
    );
}

#[test]
fn recovery_does_not_inject_citation_or_duplicate_orphans() {
    // Regression for recover_missing_content (recovery.rs): paged body lines whose
    // prose is broken up by OMML math and superscript citations used to be misjudged
    // as "missing" and prepended at body index 0, injecting citation-number strings
    // and duplicated body sentences as orphans above the abstract. See
    // tests/fixtures/edge_recovery_no_orphans.typ.
    let doc_xml = fixture_doc_xml("edge_recovery_no_orphans");

    // Collect each paragraph's plain text (w:t only).
    let para_texts: Vec<String> = common::paragraph_texts(&doc_xml);

    // No paragraph may consist solely of citation/footnote markers like "[1]".
    let is_marker_only = |t: &str| {
        let t = t.trim();
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '[' | ']' | ',' | ' '))
            && t.contains('[')
    };
    assert!(
        !para_texts.iter().any(|t| is_marker_only(t)),
        "recovery must not inject citation-marker-only orphan paragraphs: {para_texts:?}"
    );

    // The abstract must sit right after the title block, not be pushed below
    // injected orphans.
    let abstract_idx = para_texts
        .iter()
        .position(|t| t.contains("摘要"))
        .expect("abstract present");
    assert!(
        abstract_idx <= 2,
        "abstract should stay near the top (after title block), found at {abstract_idx}"
    );

    // Body sentences must appear exactly once (no recovery-injected duplicate).
    assert_eq!(
        doc_xml.matches("正文第一段").count(),
        1,
        "body sentence must not be duplicated by recovery"
    );
}

#[test]
fn recovery_dedups_heading_with_number_beyond_old_table() {
    // Regression for recover_missing_content (recovery.rs): a SHORT heading whose
    // Typst-computed number is outside the old hardcoded Chinese-numeral table
    // (一..十五) — here "十六、讨论" / "十七、综述" — was re-scraped from page
    // geometry and injected as a duplicate orphan, because the short-line (<6 char)
    // gate skipped the whitespace-cancelled full-text dedup and the numeral table
    // capped at 十五. The fix dedups heading lines against the emitted heading text
    // (which carries the same Typst number), language-agnostically — no numeral
    // table. See tests/fixtures/edge_recovery_heading_beyond_table.typ.
    let doc_xml = fixture_doc_xml("edge_recovery_heading_beyond_table");

    // Each paragraph's concatenated w:t text.
    let para_texts: Vec<String> = common::paragraph_texts(&doc_xml);

    let para_count = |needle: &str| para_texts.iter().filter(|t| t.contains(needle)).count();
    assert_eq!(
        para_count("讨论"),
        1,
        "heading '十六、讨论' must not be duplicated as a recovery orphan: {para_texts:?}"
    );
    assert_eq!(
        para_count("综述"),
        1,
        "heading '十七、综述' must not be duplicated as a recovery orphan: {para_texts:?}"
    );
}

#[test]
fn recovery_keeps_centered_enumerated_line_not_over_suppressed() {
    // Regression for recover_missing_content (recovery.rs) "site 2": the old code
    // stripped a hardcoded Chinese-numeral prefix before the CJK-projection dedup,
    // which OVER-SUPPRESSED a legitimate layout-only centered line ("三、甲乙丙，丁戊己")
    // whose number-stripped projection collided with body prose ("甲乙丙丁戊己").
    // Removing the strip keeps the numeral in the projection, so the distinct
    // centered line survives. The centered line is the only place with the
    // comma-bearing "甲乙丙，丁戊己", so its presence proves it was not deleted.
    // See tests/fixtures/edge_recovery_enumerated_centered_line.typ.
    let doc_xml = fixture_doc_xml("edge_recovery_enumerated_centered_line");
    assert!(
        doc_xml.contains("三、甲乙丙"),
        "the centered enumerated line must be preserved, not over-suppressed by recovery:\n{doc_xml}"
    );
}

#[test]
fn complex_paper_has_table_structure() {
    let doc_xml = fixture_doc_xml("complex_paper");

    // Verify w:tbl structure is present
    assert!(
        doc_xml.contains("w:tbl"),
        "document.xml should contain w:tbl element for the table"
    );
    assert!(
        doc_xml.contains("w:tr"),
        "document.xml should contain w:tr elements for table rows"
    );
    assert!(
        doc_xml.contains("w:tc"),
        "document.xml should contain w:tc elements for table cells"
    );
    assert!(
        doc_xml.contains("w:tblBorders"),
        "table should have borders defined"
    );
    // Check that table content is in cells
    assert!(
        doc_xml.contains("变量类型") || doc_xml.contains("变量名称"),
        "table cells should contain the paper's table content"
    );
}

#[test]
fn complex_paper_has_list_numbering() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify w:numPr is present for list items
    assert!(
        doc_xml.contains("w:numPr"),
        "document.xml should contain w:numPr for list items"
    );
    assert!(
        doc_xml.contains("w:ilvl"),
        "document.xml should contain w:ilvl for list level"
    );
    assert!(
        doc_xml.contains("w:numId"),
        "document.xml should contain w:numId for numbering instance"
    );

    // Verify numbering.xml exists
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n == "word/numbering.xml"),
        "docx should contain word/numbering.xml, got: {names:?}"
    );

    // Verify numbering.xml content
    let num_xml = std::io::read_to_string(reader.by_name("word/numbering.xml").unwrap()).unwrap();
    assert!(
        num_xml.contains("w:numbering"),
        "numbering.xml should have w:numbering root"
    );
    assert!(
        num_xml.contains("w:abstractNum"),
        "numbering.xml should contain abstract numbering definitions"
    );
    assert!(
        num_xml.contains("w:numFmt"),
        "numbering.xml should contain number format definitions"
    );

    // Verify content types include numbering
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("numbering"),
        "content types should reference numbering"
    );

    // Verify document rels include numbering relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("numbering"),
        "document rels should reference numbering"
    );
}

#[test]
fn end_to_end_hello_typ_to_docx() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<&str> = reader.file_names().collect();

    assert!(names.contains(&"[Content_Types].xml"));
    assert!(names.contains(&"word/document.xml"));
    assert!(names.contains(&"word/styles.xml"));
    assert!(names.contains(&"word/fontTable.xml"));

    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:document"));
    assert!(doc_xml.contains("Hello"));
    assert!(doc_xml.contains("Heading1"), "should have heading style");
    assert!(
        doc_xml.contains("w:sectPr"),
        "should have section properties"
    );
}

#[test]
fn complex_paper_has_semantic_structure() {
    let doc_xml = fixture_doc_xml("complex_paper");

    assert!(doc_xml.contains("Heading1"), "should have Heading1");
    assert!(doc_xml.contains("Heading2"), "should have Heading2");
    assert!(doc_xml.contains("<w:b/>"), "should have bold formatting");
    assert!(doc_xml.contains("w:pgMar"), "should have page margins");
    assert!(
        doc_xml.contains("数字经济"),
        "should contain Chinese paper text"
    );
}

#[test]
fn complex_paper_has_footnotes() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Verify footnotes were detected in the document model
    assert!(!doc.footnotes.is_empty(), "should have detected footnotes");
    assert_eq!(doc.footnotes.len(), 3, "complex paper has 3 footnotes");

    // Verify footnote content was extracted
    let first_fn_text: String = doc.footnotes[0]
        .content
        .iter()
        .filter_map(|i| {
            if let typort_ooxml::document::InlineElement::Text(r) = i {
                Some(r.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        first_fn_text.contains("习近平"),
        "first footnote should contain reference text, got: {first_fn_text}"
    );

    // Write to docx and verify XML structure
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Verify footnotes.xml exists in the archive
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n == "word/footnotes.xml"),
        "docx should contain word/footnotes.xml, got: {names:?}"
    );

    // Verify document.xml has footnote references
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:footnoteReference"),
        "document.xml should contain w:footnoteReference"
    );
    assert!(
        doc_xml.contains("FootnoteReference"),
        "document.xml should reference FootnoteReference style"
    );

    // Verify footnotes.xml content
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();
    assert!(
        fn_xml.contains("w:footnotes"),
        "footnotes.xml should have w:footnotes root"
    );
    assert!(
        fn_xml.contains("w:footnoteRef"),
        "footnotes.xml should contain w:footnoteRef"
    );
    assert!(
        fn_xml.contains("习近平"),
        "footnotes.xml should contain first footnote text"
    );
    assert!(
        fn_xml.contains("Schumpeter"),
        "footnotes.xml should contain second footnote text"
    );

    // Verify content types include footnotes
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("footnotes"),
        "content types should reference footnotes"
    );

    // Verify document rels include footnotes relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("footnotes"),
        "document rels should reference footnotes"
    );
}

#[test]
fn center_test_recovers_aligned_content() {
    use typort_ooxml::document::Alignment;

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/center_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // The centered text "张三  李四" should be recovered from PagedDocument
    let has_centered_authors = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            let text = p.text_content();
            (text.contains("张三") || text.contains("李四"))
                && p.alignment == Some(Alignment::Center)
        } else {
            false
        }
    });
    assert!(
        has_centered_authors,
        "center_test should recover centered author names"
    );
}

#[test]
fn complex_paper_recovers_author_info() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // The author names and institution info from #align(center) should be recovered
    let has_author = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            p.text_runs()
                .any(|r| r.text.contains("张三") || r.text.contains("李四"))
        } else {
            false
        }
    });
    assert!(
        has_author,
        "complex paper should recover author names from #align(center)"
    );

    let has_institution = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            p.text_runs()
                .any(|r| r.text.contains("某大学") || r.text.contains("经济学院"))
        } else {
            false
        }
    });
    assert!(
        has_institution,
        "complex paper should recover institution info from #align(center)"
    );
}

#[test]
fn grid_content_recovered_in_output() {
    let doc_xml = fixture_doc_xml("grid_test");

    // All grid text should appear in the output
    assert!(
        doc_xml.contains("Left column text"),
        "document.xml should contain 'Left column text' from grid"
    );
    assert!(
        doc_xml.contains("Right column text"),
        "document.xml should contain 'Right column text' from grid"
    );
    assert!(
        doc_xml.contains("Row 2 left"),
        "document.xml should contain 'Row 2 left' from grid"
    );
    assert!(
        doc_xml.contains("Row 2 right"),
        "document.xml should contain 'Row 2 right' from grid"
    );
    // Normal text after grid should also be present
    assert!(
        doc_xml.contains("Some normal text after grid"),
        "document.xml should contain text after the grid"
    );
}

#[test]
fn grid_multi_column_has_tab_stops() {
    let doc_xml = fixture_doc_xml("grid_test");

    // Multi-column grid lines should produce tab stops in the XML
    assert!(
        doc_xml.contains("<w:tab"),
        "grid layout should produce tab elements in the output"
    );
}

#[test]
fn grid_document_model_has_tab_inlines() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/grid_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with Tab inline elements (multi-column grid lines)
    let has_tab = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines.iter().any(|i| matches!(i, InlineElement::Tab))
        } else {
            false
        }
    });
    assert!(
        has_tab,
        "document model should have Tab inline elements for multi-column grid lines"
    );

    // Paragraphs with Tab should also have tab_stops set
    let has_tab_stops = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            !p.tab_stops.is_empty()
        } else {
            false
        }
    });
    assert!(
        has_tab_stops,
        "document model should have tab_stops for multi-column grid lines"
    );
}

#[test]
fn footnote_and_table_not_recovered_as_body_orphans() {
    // The footnote body must live in the footnote zone, not be scraped into the
    // document body; and no horizontal rule may be invented from the footnote
    // separator or the table's border lines (the source declares no #line()).
    // See tests/fixtures/edge_footnote_table_no_orphan.typ.
    let doc_xml = fixture_doc_xml("edge_footnote_table_no_orphan");
    assert!(
        !doc_xml.contains("must stay in the footnote zone"),
        "footnote body must not be duplicated into the document body:\n{doc_xml}"
    );
    assert!(
        !doc_xml.contains("<w:pBdr>"),
        "no horizontal rule should be invented without a source #line():\n{doc_xml}"
    );
}

#[test]
fn wrapped_table_row_not_recovered_as_orphan() {
    // A table cell that wraps in a narrow column must not be re-scraped into the
    // body as a tab-stop orphan row (the wrap truncates the cell so text-dedup
    // misses it). "主成分分析法" lives only in complex_paper's measurement table.
    let doc_xml = fixture_doc_xml("complex_paper");
    assert_eq!(
        doc_xml.matches("主成分分析法").count(),
        1,
        "table cell text must appear once (in the table), not duplicated as a recovered orphan"
    );
}
