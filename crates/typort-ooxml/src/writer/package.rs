use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};

use super::footnotes::write_footnote_pr;
use super::image::image_rel_id;
use super::{DocParts, RelKind};
use crate::document::{Document, DocumentStyle, ImageData, ImageFormat};

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

pub(crate) fn write_value_element<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
) -> io::Result<()> {
    writer
        .create_element(name)
        .with_attribute(("w:val", value))
        .write_empty()?;
    Ok(())
}

pub(crate) fn write_font_triple<W: Write>(
    writer: &mut Writer<W>,
    ascii: &str,
    east_asia: &str,
    hint_east_asia: bool,
) -> io::Result<()> {
    let mut fonts = writer
        .create_element("w:rFonts")
        .with_attribute(("w:ascii", ascii))
        .with_attribute(("w:hAnsi", ascii))
        .with_attribute(("w:eastAsia", east_asia));
    if hint_east_asia {
        fonts = fonts.with_attribute(("w:hint", "eastAsia"));
    }
    fonts.write_empty()?;
    Ok(())
}

pub(crate) fn write_size_pair<W: Write>(writer: &mut Writer<W>, size: &str) -> io::Result<()> {
    write_value_element(writer, "w:sz", size)?;
    write_value_element(writer, "w:szCs", size)
}

pub(crate) fn write_language_pair<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    latin: &str,
    east_asia: &str,
) -> io::Result<()> {
    writer
        .create_element(name)
        .with_attribute(("w:val", latin))
        .with_attribute(("w:eastAsia", east_asia))
        .write_empty()?;
    Ok(())
}

pub(crate) fn write_indentation<W: Write>(
    writer: &mut Writer<W>,
    left: Option<&str>,
    hanging: Option<&str>,
    first_line_chars: Option<&str>,
    first_line: Option<&str>,
) -> io::Result<()> {
    let mut indentation = writer.create_element("w:ind");
    if let Some(value) = left {
        indentation = indentation.with_attribute(("w:left", value));
    }
    if let Some(value) = hanging {
        indentation = indentation.with_attribute(("w:hanging", value));
    }
    if let Some(value) = first_line_chars {
        indentation = indentation.with_attribute(("w:firstLineChars", value));
    }
    if let Some(value) = first_line {
        indentation = indentation.with_attribute(("w:firstLine", value));
    }
    indentation.write_empty()?;
    Ok(())
}

pub(crate) fn two_em_hanging_twips(size_half_pt: u32) -> u32 {
    size_half_pt * 10 * 2
}

pub(super) fn generate_content_types(
    writer: &mut Writer<&mut Vec<u8>>,
    parts: &DocParts,
    images: &[&ImageData],
) -> io::Result<()> {
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
            for format in [ImageFormat::Png, ImageFormat::Jpeg] {
                if images.iter().any(|image| image.format == format) {
                    w.create_element("Default")
                        .with_attribute(("Extension", format.extension()))
                        .with_attribute(("ContentType", format.content_type()))
                        .write_empty()?;
                }
            }
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
            if parts.has(RelKind::Footnotes) {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/footnotes.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"))
                    .write_empty()?;
            }
            if parts.has(RelKind::Numbering) {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/numbering.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"))
                    .write_empty()?;
            }
            w.create_element("Override")
                .with_attribute(("PartName", "/word/settings.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"))
                .write_empty()?;
            if parts.has(RelKind::Header) {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/header1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"))
                    .write_empty()?;
            }
            if parts.has(RelKind::Footer) {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/footer1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"))
                    .write_empty()?;
            }
            w.create_element("Override")
                .with_attribute(("PartName", "/docProps/core.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-package.core-properties+xml"))
                .write_empty()?;
            if parts.has(RelKind::Bibliography) {
                w.create_element("Override")
                    .with_attribute(("PartName", "/customXml/itemProps1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.customXmlProperties+xml"))
                    .write_empty()?;
            }
            Ok(())
        })?;
    Ok(())
}

pub(super) fn generate_rels(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
    writer
        .create_element("Relationships")
        .with_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships"))
        .write_inner_content(|w| {
            w.create_element("Relationship")
                .with_attribute(("Id", "rId1"))
                .with_attribute(("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"))
                .with_attribute(("Target", "word/document.xml"))
                .write_empty()?;
            w.create_element("Relationship")
                .with_attribute(("Id", "rId2"))
                .with_attribute(("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"))
                .with_attribute(("Target", "docProps/core.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

pub(super) fn generate_document_rels(
    writer: &mut Writer<&mut Vec<u8>>,
    parts: &DocParts,
    images: &[&ImageData],
) -> io::Result<()> {
    writer
        .create_element("Relationships")
        .with_attribute((
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        ))
        .write_inner_content(|w| {
            // Fixed parts, in the order that defines every rId (see `RelKind`).
            for (idx, kind) in parts.relationships.iter().enumerate() {
                let rid = format!("rId{}", idx + 1);
                let (rel_type, target) = kind.type_and_target();
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute(("Type", rel_type.as_str()))
                    .with_attribute(("Target", target))
                    .write_empty()?;
            }

            // Image relationships
            for (idx, img) in images.iter().enumerate() {
                let rid = image_rel_id(idx + 1, parts);
                let ext = img.format.extension();
                let target = format!("media/image{}.{ext}", idx + 1);
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                    ))
                    .with_attribute(("Target", target.as_str()))
                    .write_empty()?;
            }
            Ok(())
        })?;
    Ok(())
}

pub(super) fn generate_settings(
    writer: &mut Writer<&mut Vec<u8>>,
    style: &DocumentStyle,
) -> io::Result<()> {
    writer
        .create_element("w:settings")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:m",
            "http://schemas.openxmlformats.org/officeDocument/2006/math",
        ))
        .write_inner_content(|w| {
            write_footnote_pr(w, &style.footnote_format)?;
            w.create_element("w:compat").write_inner_content(|c| {
                c.create_element("w:useFELayout").write_empty()?;
                c.create_element("w:compatSetting")
                    .with_attribute(("w:name", "compatibilityMode"))
                    .with_attribute(("w:uri", "http://schemas.microsoft.com/office/word"))
                    .with_attribute(("w:val", "15"))
                    .write_empty()?;
                Ok(())
            })?;
            write_language_pair(
                w,
                "w:themeFontLang",
                style.lang_latin.as_str(),
                style.lang_east_asia.as_str(),
            )?;
            w.create_element("m:mathPr").write_inner_content(|m| {
                m.create_element("m:mathFont")
                    .with_attribute(("m:val", "Cambria Math"))
                    .write_empty()?;
                m.create_element("m:brkBin")
                    .with_attribute(("m:val", "before"))
                    .write_empty()?;
                m.create_element("m:brkBinSub")
                    .with_attribute(("m:val", "--"))
                    .write_empty()?;
                m.create_element("m:smallFrac")
                    .with_attribute(("m:val", "0"))
                    .write_empty()?;
                m.create_element("m:dispDef").write_empty()?;
                m.create_element("m:lMargin")
                    .with_attribute(("m:val", "0"))
                    .write_empty()?;
                m.create_element("m:rMargin")
                    .with_attribute(("m:val", "0"))
                    .write_empty()?;
                m.create_element("m:defJc")
                    .with_attribute(("m:val", "centerGroup"))
                    .write_empty()?;
                m.create_element("m:wrapIndent")
                    .with_attribute(("m:val", "1440"))
                    .write_empty()?;
                m.create_element("m:intLim")
                    .with_attribute(("m:val", "subSup"))
                    .write_empty()?;
                m.create_element("m:naryLim")
                    .with_attribute(("m:val", "undOvr"))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}
pub(super) fn generate_core_properties(
    writer: &mut Writer<&mut Vec<u8>>,
    doc: &Document,
) -> io::Result<()> {
    writer
        .create_element("cp:coreProperties")
        .with_attribute((
            "xmlns:cp",
            "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        ))
        .with_attribute(("xmlns:dc", "http://purl.org/dc/elements/1.1/"))
        .with_attribute(("xmlns:dcterms", "http://purl.org/dc/terms/"))
        .with_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"))
        .write_inner_content(|w| {
            if let Some(title) = &doc.metadata.title {
                w.create_element("dc:title")
                    .write_text_content(BytesText::new(title))?;
            }
            if let Some(author) = &doc.metadata.author {
                w.create_element("dc:creator")
                    .write_text_content(BytesText::new(author))?;
            }
            w.create_element("dcterms:created")
                .with_attribute(("xsi:type", "dcterms:W3CDTF"))
                .write_text_content(BytesText::new(&doc.metadata.created_time()))?;
            Ok(())
        })?;
    Ok(())
}
