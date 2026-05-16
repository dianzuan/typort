use std::io::{self, Seek, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{BlockElement, Document, ParagraphStyle};
use crate::styles;

/// Write a Document to a .docx file (ZIP archive) into the given writer.
///
/// # Errors
/// Returns an error if XML serialization or ZIP writing fails.
pub fn write_docx<W: Write + Seek>(
    doc: &Document,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(&xml_part(generate_content_types)?)?;

    zip.start_file("_rels/.rels", options)?;
    zip.write_all(&xml_part(generate_rels)?)?;

    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(&xml_part(generate_document_rels)?)?;

    zip.start_file("word/styles.xml", options)?;
    zip.write_all(&xml_part(styles::generate_styles)?)?;

    zip.start_file("word/fontTable.xml", options)?;
    zip.write_all(&xml_part(styles::generate_font_table)?)?;

    zip.start_file("word/document.xml", options)?;
    zip.write_all(&xml_part(|w| generate_document_xml(w, doc))?)?;

    zip.finish()?;
    Ok(())
}

pub(crate) fn xml_part(
    build: impl FnOnce(&mut Writer<&mut Vec<u8>>) -> io::Result<()>,
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;
    build(&mut writer)?;
    Ok(buf)
}

fn generate_content_types(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
            w.create_element("Override")
                .with_attribute(("PartName", "/word/styles.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"))
                .write_empty()?;
            w.create_element("Override")
                .with_attribute(("PartName", "/word/fontTable.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn generate_rels(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
    Ok(())
}

fn generate_document_rels(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
    writer
        .create_element("Relationships")
        .with_attribute((
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        ))
        .write_inner_content(|w| {
            w.create_element("Relationship")
                .with_attribute(("Id", "rId1"))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                ))
                .with_attribute(("Target", "styles.xml"))
                .write_empty()?;
            w.create_element("Relationship")
                .with_attribute(("Id", "rId2"))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable",
                ))
                .with_attribute(("Target", "fontTable.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn generate_document_xml(writer: &mut Writer<&mut Vec<u8>>, doc: &Document) -> io::Result<()> {
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
                write_section_properties(body_w, &doc.page_settings)?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_section_properties<W: Write>(
    writer: &mut Writer<W>,
    settings: &crate::document::PageSettings,
) -> io::Result<()> {
    writer.create_element("w:sectPr").write_inner_content(|w| {
        w.create_element("w:pgSz")
            .with_attribute(("w:w", settings.width_twips.to_string().as_str()))
            .with_attribute(("w:h", settings.height_twips.to_string().as_str()))
            .write_empty()?;
        w.create_element("w:pgMar")
            .with_attribute(("w:top", settings.margin_top.to_string().as_str()))
            .with_attribute(("w:right", settings.margin_right.to_string().as_str()))
            .with_attribute(("w:bottom", settings.margin_bottom.to_string().as_str()))
            .with_attribute(("w:left", settings.margin_left.to_string().as_str()))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
}

fn write_paragraph<W: Write>(
    writer: &mut Writer<W>,
    para: &crate::document::Paragraph,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        if let Some(style) = &para.style {
            w.create_element("w:pPr").write_inner_content(|ppr| {
                let style_id = match style {
                    ParagraphStyle::Heading(n) => format!("Heading{n}"),
                    ParagraphStyle::Normal => "Normal".to_string(),
                };
                ppr.create_element("w:pStyle")
                    .with_attribute(("w:val", style_id.as_str()))
                    .write_empty()?;
                Ok(())
            })?;
        }
        for run in &para.runs {
            write_run(w, run)?;
        }
        Ok(())
    })?;
    Ok(())
}

fn write_run<W: Write>(writer: &mut Writer<W>, run: &crate::document::Run) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        if run.bold || run.italic {
            w.create_element("w:rPr").write_inner_content(|rpr| {
                if run.bold {
                    rpr.create_element("w:b").write_empty()?;
                }
                if run.italic {
                    rpr.create_element("w:i").write_empty()?;
                }
                Ok(())
            })?;
        }
        w.create_element("w:t")
            .with_attribute(("xml:space", "preserve"))
            .write_text_content(BytesText::new(&run.text))?;
        Ok(())
    })?;
    Ok(())
}
