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
