use std::io::{self, Seek, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{BlockElement, Document};

/// Write a Document to a .docx file (ZIP archive) into the given writer.
pub fn write_docx<W: Write + Seek>(
    doc: &Document,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(generate_content_types()?.as_bytes())?;

    // _rels/.rels
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(generate_rels()?.as_bytes())?;

    // word/_rels/document.xml.rels
    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(generate_document_rels()?.as_bytes())?;

    // word/document.xml
    zip.start_file("word/document.xml", options)?;
    zip.write_all(generate_document_xml(doc)?.as_bytes())?;

    zip.finish()?;
    Ok(())
}

fn generate_content_types() -> io::Result<String> {
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    writer
        .create_element("Types")
        .with_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types"))
        .write_inner_content(|w| {
            w.create_element("Default")
                .with_attribute(("Extension", "rels"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-package.relationships+xml"))
                .write_empty()?;
            w.create_element("Default")
                .with_attribute(("Extension", "xml"))
                .with_attribute(("ContentType", "application/xml"))
                .write_empty()?;
            w.create_element("Override")
                .with_attribute(("PartName", "/word/document.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"))
                .write_empty()?;
            Ok(())
        })?;

    Ok(String::from_utf8(buf).unwrap())
}

fn generate_rels() -> io::Result<String> {
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    writer
        .create_element("Relationships")
        .with_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships"))
        .write_inner_content(|w| {
            w.create_element("Relationship")
                .with_attribute(("Id", "rId1"))
                .with_attribute(("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"))
                .with_attribute(("Target", "word/document.xml"))
                .write_empty()?;
            Ok(())
        })?;

    Ok(String::from_utf8(buf).unwrap())
}

fn generate_document_rels() -> io::Result<String> {
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    writer
        .create_element("Relationships")
        .with_attribute((
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        ))
        .write_inner_content(|_w| Ok(()))?;

    Ok(String::from_utf8(buf).unwrap())
}

fn generate_document_xml(doc: &Document) -> io::Result<String> {
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    writer
        .create_element("w:document")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ))
        .write_inner_content(|w| {
            w.create_element("w:body").write_inner_content(|body_w| {
                for element in &doc.body.elements {
                    match element {
                        BlockElement::Paragraph(para) => {
                            write_paragraph(body_w, para)?;
                        }
                    }
                }
                Ok(())
            })?;
            Ok(())
        })?;

    Ok(String::from_utf8(buf).unwrap())
}

fn write_paragraph<W: Write>(
    writer: &mut Writer<W>,
    para: &crate::document::Paragraph,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        for run in &para.runs {
            w.create_element("w:r").write_inner_content(|rw| {
                rw.create_element("w:t")
                    .with_attribute(("xml:space", "preserve"))
                    .write_text_content(BytesText::new(&run.text))?;
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(())
}
