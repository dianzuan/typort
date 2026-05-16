use std::io::Cursor;
use std::path::Path;

#[test]
fn end_to_end_hello_typ_to_docx() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let paged = typort_core::compile(&world).unwrap();
    let doc = typort_core::convert::convert_document(&paged);

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<&str> = reader.file_names().collect();

    assert!(names.contains(&"[Content_Types].xml"), "missing [Content_Types].xml");
    assert!(names.contains(&"word/document.xml"), "missing word/document.xml");
    assert!(names.contains(&"_rels/.rels"), "missing _rels/.rels");

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:document"), "should contain w:document element");
    assert!(doc_xml.contains("Hello"), "should contain text from hello.typ");
}
