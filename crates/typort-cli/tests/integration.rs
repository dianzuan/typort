use std::io::Cursor;
use std::path::Path;

#[test]
fn end_to_end_hello_typ_to_docx() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert_html(&world).unwrap();

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_html(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

    // Verify footnotes were detected in the document model
    assert!(!doc.footnotes.is_empty(), "should have detected footnotes");
    assert_eq!(doc.footnotes.len(), 3, "complex paper has 3 footnotes");

    // Verify footnote content was extracted
    let first_fn_text: String = doc.footnotes[0]
        .content
        .iter()
        .map(|r| r.text.as_str())
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
fn italic_text_produces_w_i_element() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/italic_test.typ")).unwrap();
    let doc = typort_core::convert_html(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("<w:i/>"),
        "should have italic formatting element <w:i/>"
    );
    assert!(
        doc_xml.contains("emphasized text"),
        "should contain italic text content"
    );
}
