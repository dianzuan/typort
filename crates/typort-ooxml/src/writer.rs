use std::io::{self, Seek, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{
    Alignment, BlockElement, Document, FootnoteFormat, ImageData, ImageFormat, InlineElement,
    PageNumberFormat, ParagraphStyle, SectionBreak, SectionBreakType, Table,
};
use crate::styles;

/// Tracks which optional document parts are present, used for rId assignment
/// and conditional XML generation.
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct DocParts {
    footnotes: bool,
    numbering: bool,
    header: bool,
    footer: bool,
    bibliography: bool,
    content_width_twips: u32,
}

/// Write a Document to a .docx file (ZIP archive) into the given writer.
///
/// # Errors
/// Returns an error if XML serialization or ZIP writing fails.
pub fn write_docx<W: Write + Seek>(
    doc: &Document,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let ps = &doc.page_settings;
    let parts = DocParts {
        footnotes: !doc.footnotes.is_empty(),
        numbering: doc_has_lists(doc),
        header: doc.header.is_some(),
        footer: doc.footer.is_some() || doc.page_numbering.is_some(),
        bibliography: !doc.citation_sources.is_empty(),
        content_width_twips: ps
            .width_twips
            .saturating_sub(ps.margin_left + ps.margin_right),
    };
    let images = collect_images(doc);
    let has_images = !images.is_empty();

    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(&xml_part(|w| generate_content_types(w, parts, &images))?)?;

    zip.start_file("_rels/.rels", options)?;
    zip.write_all(&xml_part(generate_rels)?)?;

    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(&xml_part(|w| generate_document_rels(w, parts, &images))?)?;

    zip.start_file("word/styles.xml", options)?;
    zip.write_all(&xml_part(|w| {
        styles::generate_styles(w, parts.footnotes, &doc.style)
    })?)?;

    zip.start_file("word/fontTable.xml", options)?;
    zip.write_all(&xml_part(|w| styles::generate_font_table(w, &doc.style))?)?;

    zip.start_file("word/settings.xml", options)?;
    zip.write_all(&xml_part(|w| generate_settings(w, &doc.style))?)?;

    zip.start_file("word/document.xml", options)?;
    zip.write_all(&xml_part(|w| generate_document_xml(w, doc, has_images))?)?;

    if parts.footnotes {
        zip.start_file("word/footnotes.xml", options)?;
        zip.write_all(&xml_part(|w| generate_footnotes_xml(w, doc))?)?;
    }

    if parts.numbering {
        zip.start_file("word/numbering.xml", options)?;
        zip.write_all(&xml_part(|w| generate_numbering_xml(w, doc))?)?;
    }

    // Write header XML
    if let Some(header) = &doc.header {
        zip.start_file("word/header1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_header_xml(w, header, &doc.style))?)?;
    }

    // Write footer XML — either a PAGE field for page numbering, or static content
    if let Some(pg_fmt) = &doc.page_numbering {
        let fmt = pg_fmt.clone();
        zip.start_file("word/footer1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_page_number_footer_xml(w, &fmt))?)?;
    } else if let Some(footer) = &doc.footer {
        zip.start_file("word/footer1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_footer_xml(w, footer, &doc.style))?)?;
    }

    // Write image files to word/media/
    for (idx, img) in images.iter().enumerate() {
        let n = idx + 1;
        let ext = match img.format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        };
        let path = format!("word/media/image{n}.{ext}");
        zip.start_file(path, options)?;
        zip.write_all(&img.bytes)?;
    }

    // Always write docProps/core.xml with metadata
    zip.start_file("docProps/core.xml", options)?;
    zip.write_all(&xml_part(|w| generate_core_properties(w, doc))?)?;

    // Custom XML bibliography data source
    if parts.bibliography {
        zip.start_file("customXml/item1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_custom_xml_sources(w, &doc.citation_sources))?)?;

        zip.start_file("customXml/itemProps1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_custom_xml_item_props(w, &doc.citation_sources))?)?;

        zip.start_file("customXml/_rels/item1.xml.rels", options)?;
        zip.write_all(&xml_part(generate_custom_xml_rels)?)?;
    }

    zip.finish()?;
    Ok(())
}

/// Check if the document contains any list items (paragraphs with `list_info` set).
fn doc_has_lists(doc: &Document) -> bool {
    doc.body.elements.iter().any(|el| match el {
        BlockElement::Paragraph(p) => p.list_info.is_some(),
        BlockElement::Table(_) => false,
        BlockElement::BibliographyBlock { paragraphs } => {
            paragraphs.iter().any(|p| p.list_info.is_some())
        }
    })
}

/// Collect all images from the document model in order of appearance.
/// Returns a `Vec<&ImageData>` where the index corresponds to the 1-based image number.
fn collect_images(doc: &Document) -> Vec<&ImageData> {
    let mut images = Vec::new();
    for el in &doc.body.elements {
        match el {
            BlockElement::Paragraph(p) => collect_images_from_para(p, &mut images),
            BlockElement::Table(t) => {
                collect_images_from_table(t, &mut images);
            }
            BlockElement::BibliographyBlock { paragraphs } => {
                for p in paragraphs {
                    collect_images_from_para(p, &mut images);
                }
            }
        }
    }
    images
}

fn collect_images_from_para<'a>(
    para: &'a crate::document::Paragraph,
    images: &mut Vec<&'a ImageData>,
) {
    for inline in &para.inlines {
        if let InlineElement::Image(img) = inline {
            images.push(img);
        }
    }
}

fn collect_images_from_table<'a>(
    table: &'a crate::document::Table,
    images: &mut Vec<&'a ImageData>,
) {
    for row in &table.rows {
        for cell in &row.cells {
            // Check structured content first (includes nested tables)
            if cell.content.is_empty() {
                for para in &cell.paragraphs {
                    collect_images_from_para(para, images);
                }
            } else {
                for item in &cell.content {
                    match item {
                        crate::document::CellContent::Paragraph(p) => {
                            collect_images_from_para(p, images);
                        }
                        crate::document::CellContent::Table(t) => {
                            collect_images_from_table(t, images);
                        }
                    }
                }
            }
        }
    }
}

/// Compute the relationship ID for an image given its 1-based index.
/// Image relationship IDs start after the fixed relationships (styles, fontTable,
/// footnotes, numbering, settings, header, footer).
fn image_rel_id(image_index: usize, parts: DocParts) -> String {
    // Base count: rId1=styles, rId2=fontTable
    let mut base = 2;
    if parts.footnotes {
        base += 1;
    }
    if parts.numbering {
        base += 1;
    }
    base += 1; // settings
    if parts.header {
        base += 1;
    }
    if parts.footer {
        base += 1;
    }
    if parts.bibliography {
        base += 1;
    }
    format!("rId{}", base + image_index)
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

fn generate_content_types(
    writer: &mut Writer<&mut Vec<u8>>,
    parts: DocParts,
    images: &[&ImageData],
) -> io::Result<()> {
    // Determine which image extensions are used
    let has_png = images.iter().any(|i| i.format == ImageFormat::Png);
    let has_jpeg = images.iter().any(|i| i.format == ImageFormat::Jpeg);

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
            if has_png {
                w.create_element("Default")
                    .with_attribute(("Extension", "png"))
                    .with_attribute(("ContentType", "image/png"))
                    .write_empty()?;
            }
            if has_jpeg {
                w.create_element("Default")
                    .with_attribute(("Extension", "jpg"))
                    .with_attribute(("ContentType", "image/jpeg"))
                    .write_empty()?;
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
            if parts.footnotes {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/footnotes.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"))
                    .write_empty()?;
            }
            if parts.numbering {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/numbering.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"))
                    .write_empty()?;
            }
            w.create_element("Override")
                .with_attribute(("PartName", "/word/settings.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"))
                .write_empty()?;
            if parts.header {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/header1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"))
                    .write_empty()?;
            }
            if parts.footer {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/footer1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"))
                    .write_empty()?;
            }
            w.create_element("Override")
                .with_attribute(("PartName", "/docProps/core.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-package.core-properties+xml"))
                .write_empty()?;
            if parts.bibliography {
                w.create_element("Override")
                    .with_attribute(("PartName", "/customXml/itemProps1.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.customXmlProperties+xml"))
                    .write_empty()?;
            }
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
            w.create_element("Relationship")
                .with_attribute(("Id", "rId2"))
                .with_attribute(("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"))
                .with_attribute(("Target", "docProps/core.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn generate_document_rels(
    writer: &mut Writer<&mut Vec<u8>>,
    parts: DocParts,
    images: &[&ImageData],
) -> io::Result<()> {
    writer
        .create_element("Relationships")
        .with_attribute((
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        ))
        .write_inner_content(|w| {
            // Use a counter to assign sequential rId values
            let mut next_id = 1_usize;

            // rId1 = styles
            let rid = format!("rId{next_id}");
            next_id += 1;
            w.create_element("Relationship")
                .with_attribute(("Id", rid.as_str()))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                ))
                .with_attribute(("Target", "styles.xml"))
                .write_empty()?;

            // rId2 = fontTable
            let rid = format!("rId{next_id}");
            next_id += 1;
            w.create_element("Relationship")
                .with_attribute(("Id", rid.as_str()))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable",
                ))
                .with_attribute(("Target", "fontTable.xml"))
                .write_empty()?;

            if parts.footnotes {
                let rid = format!("rId{next_id}");
                next_id += 1;
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
                    ))
                    .with_attribute(("Target", "footnotes.xml"))
                    .write_empty()?;
            }

            if parts.numbering {
                let rid = format!("rId{next_id}");
                next_id += 1;
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
                    ))
                    .with_attribute(("Target", "numbering.xml"))
                    .write_empty()?;
            }

            // settings
            let rid = format!("rId{next_id}");
            next_id += 1;
            w.create_element("Relationship")
                .with_attribute(("Id", rid.as_str()))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings",
                ))
                .with_attribute(("Target", "settings.xml"))
                .write_empty()?;

            // header
            if parts.header {
                let rid = format!("rId{next_id}");
                next_id += 1;
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
                    ))
                    .with_attribute(("Target", "header1.xml"))
                    .write_empty()?;
            }

            // footer
            if parts.footer {
                let rid = format!("rId{next_id}");
                next_id += 1; // keep counter consistent for image_rel_id parity
                let _ = next_id;
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer",
                    ))
                    .with_attribute(("Target", "footer1.xml"))
                    .write_empty()?;
            }

            // Bibliography custom XML relationship
            if parts.bibliography {
                let rid = format!("rId{next_id}");
                next_id += 1;
                w.create_element("Relationship")
                    .with_attribute(("Id", rid.as_str()))
                    .with_attribute(("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml"))
                    .with_attribute(("Target", "../customXml/item1.xml"))
                    .write_empty()?;
            }

            // Image relationships
            for (idx, img) in images.iter().enumerate() {
                let rid = image_rel_id(idx + 1, parts);
                let ext = match img.format {
                    ImageFormat::Png => "png",
                    ImageFormat::Jpeg => "jpg",
                };
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

fn generate_settings(
    writer: &mut Writer<&mut Vec<u8>>,
    style: &crate::document::DocumentStyle,
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
            w.create_element("w:themeFontLang")
                .with_attribute(("w:val", style.lang_latin.as_str()))
                .with_attribute(("w:eastAsia", style.lang_east_asia.as_str()))
                .write_empty()?;
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

/// Compute the relationship ID for a header or footer.
/// Headers/footers come after settings in the relationship chain.
fn header_footer_rel_id(is_header: bool, parts: DocParts) -> String {
    // Base count: rId1=styles, rId2=fontTable
    let mut id = 2;
    if parts.footnotes {
        id += 1;
    }
    if parts.numbering {
        id += 1;
    }
    id += 1; // settings
    if is_header {
        // header is the next one after settings
        id += 1;
    } else {
        // footer comes after header (if present)
        if parts.header {
            id += 1;
        }
        id += 1;
    }
    format!("rId{id}")
}

fn generate_document_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    doc: &Document,
    has_images: bool,
) -> io::Result<()> {
    let mut elem = writer
        .create_element("w:document")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ))
        .with_attribute((
            "xmlns:m",
            "http://schemas.openxmlformats.org/officeDocument/2006/math",
        ));
    if has_images {
        elem = elem
            .with_attribute((
                "xmlns:wp",
                "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
            ))
            .with_attribute((
                "xmlns:a",
                "http://schemas.openxmlformats.org/drawingml/2006/main",
            ))
            .with_attribute((
                "xmlns:pic",
                "http://schemas.openxmlformats.org/drawingml/2006/picture",
            ));
    }
    // We need to track image index state across paragraphs for the writer.
    // Use a Cell to allow mutation inside the closure.
    let image_counter = std::cell::Cell::new(0_usize);
    let citation_id_counter = std::cell::Cell::new(1_000_000_u32);
    let ps = &doc.page_settings;
    let parts = DocParts {
        footnotes: !doc.footnotes.is_empty(),
        numbering: doc_has_lists(doc),
        header: doc.header.is_some(),
        footer: doc.footer.is_some() || doc.page_numbering.is_some(),
        bibliography: !doc.citation_sources.is_empty(),
        content_width_twips: ps
            .width_twips
            .saturating_sub(ps.margin_left + ps.margin_right),
    };
    elem.write_inner_content(|w| {
        w.create_element("w:body").write_inner_content(|body_w| {
            for element in &doc.body.elements {
                match element {
                    BlockElement::Paragraph(para) => {
                        write_paragraph(
                            body_w,
                            para,
                            &doc.style.footnote_format,
                            &doc.style,
                            parts,
                            &image_counter,
                            &citation_id_counter,
                        )?;
                    }
                    BlockElement::Table(table) => {
                        write_table(
                            body_w,
                            table,
                            &doc.style.footnote_format,
                            &doc.style,
                            parts,
                            &image_counter,
                            &citation_id_counter,
                        )?;
                    }
                    BlockElement::BibliographyBlock { paragraphs } => {
                        write_bibliography_sdt(
                            body_w,
                            paragraphs,
                            &doc.style.footnote_format,
                            &doc.style,
                            parts,
                            &image_counter,
                            &citation_id_counter,
                        )?;
                    }
                }
            }
            write_section_properties(
                body_w,
                &doc.page_settings,
                &doc.style,
                parts,
                doc.page_numbering.as_ref(),
            )?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

fn write_section_properties<W: Write>(
    writer: &mut Writer<W>,
    settings: &crate::document::PageSettings,
    style: &crate::document::DocumentStyle,
    parts: DocParts,
    page_numbering: Option<&PageNumberFormat>,
) -> io::Result<()> {
    writer.create_element("w:sectPr").write_inner_content(|w| {
        // Header reference
        if parts.header {
            let rid = header_footer_rel_id(true, parts);
            w.create_element("w:headerReference")
                .with_attribute(("w:type", "default"))
                .with_attribute(("r:id", rid.as_str()))
                .write_empty()?;
        }
        // Footer reference
        if parts.footer {
            let rid = header_footer_rel_id(false, parts);
            w.create_element("w:footerReference")
                .with_attribute(("w:type", "default"))
                .with_attribute(("r:id", rid.as_str()))
                .write_empty()?;
        }
        write_footnote_pr(w, &style.footnote_format)?;
        // Page number format (w:pgNumType)
        if let Some(fmt) = page_numbering {
            let fmt_val = page_number_format_val(fmt);
            w.create_element("w:pgNumType")
                .with_attribute(("w:fmt", fmt_val))
                .write_empty()?;
        }
        write_section_page_settings(w, settings, style)?;
        Ok(())
    })?;
    Ok(())
}

/// Write `w:footnotePr` element with optional circled-number format and per-page restart.
fn write_footnote_pr<W: Write>(
    writer: &mut Writer<W>,
    format: &FootnoteFormat,
) -> io::Result<()> {
    writer
        .create_element("w:footnotePr")
        .write_inner_content(|fp| {
            if *format == FootnoteFormat::CircledNumber {
                fp.create_element("w:numFmt")
                    .with_attribute(("w:val", "decimalEnclosedCircle"))
                    .write_empty()?;
            }
            fp.create_element("w:numRestart")
                .with_attribute(("w:val", "eachPage"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

/// Write page size, margins, columns, and document grid for a section.
fn write_section_page_settings<W: Write>(
    w: &mut Writer<W>,
    settings: &crate::document::PageSettings,
    style: &crate::document::DocumentStyle,
) -> io::Result<()> {
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
    // Columns
    if let Some(cols) = settings.columns.filter(|&c| c > 1) {
        let num = cols.to_string();
        let space = settings.column_spacing.unwrap_or(720).to_string();
        w.create_element("w:cols")
            .with_attribute(("w:num", num.as_str()))
            .with_attribute(("w:space", space.as_str()))
            .write_empty()?;
    }
    // Document grid: use default (no grid) since line spacing is controlled
    // precisely via w:lineRule="atLeast". type="lines" would add grid pitch
    // on top of paragraph spacing, doubling the line height.
    let line_pitch_str = style.line_spacing.to_string();
    w.create_element("w:docGrid")
        .with_attribute(("w:linePitch", line_pitch_str.as_str()))
        .write_empty()?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn write_paragraph<W: Write>(
    writer: &mut Writer<W>,
    para: &crate::document::Paragraph,
    fn_format: &crate::document::FootnoteFormat,
    doc_style: &crate::document::DocumentStyle,
    parts: DocParts,
    image_counter: &std::cell::Cell<usize>,
    citation_id_counter: &std::cell::Cell<u32>,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        let has_style = para.style.is_some();
        let has_list = para.list_info.is_some();
        let has_alignment = para.alignment.is_some();
        let has_left_indent = para.left_indent.is_some();
        let has_code_block = para.code_block;
        let has_section_break = para.section_break.is_some();
        // Determine if we need to suppress the inherited first-line indent
        let suppress_indent = para.suppress_indent
            || (has_alignment
                && matches!(para.alignment, Some(Alignment::Center | Alignment::Right)));
        // Check if this paragraph has a numbered equation (needs right tab stop)
        let has_eq_number = para.inlines.iter().any(|i| {
            matches!(
                i,
                InlineElement::Math {
                    equation_number: Some(_),
                    ..
                }
            )
        });
        let has_hanging = para.hanging_indent;
        let has_hrule = para.horizontal_rule;
        let has_tab_stops = !para.tab_stops.is_empty();
        let has_spacing = para.spacing_before.is_some();
        if has_style
            || has_list
            || has_alignment
            || suppress_indent
            || has_eq_number
            || has_hanging
            || has_left_indent
            || has_code_block
            || has_section_break
            || has_hrule
            || has_tab_stops
            || has_spacing
        {
            w.create_element("w:pPr").write_inner_content(|ppr| {
                // Horizontal rule: emit bottom border
                if has_hrule {
                    ppr.create_element("w:pBdr").write_inner_content(|bdr| {
                        bdr.create_element("w:bottom")
                            .with_attribute(("w:val", "single"))
                            .with_attribute(("w:sz", "6"))
                            .with_attribute(("w:space", "1"))
                            .with_attribute(("w:color", "auto"))
                            .write_empty()?;
                        Ok(())
                    })?;
                }
                if has_code_block {
                    ppr.create_element("w:pStyle")
                        .with_attribute(("w:val", "CodeBlock"))
                        .write_empty()?;
                } else if let Some(style) = &para.style {
                    let style_id = match style {
                        ParagraphStyle::Heading(n) => format!("Heading{n}"),
                        ParagraphStyle::Normal => "Normal".to_string(),
                    };
                    ppr.create_element("w:pStyle")
                        .with_attribute(("w:val", style_id.as_str()))
                        .write_empty()?;
                }
                if let Some(li) = &para.list_info {
                    let id_str = li.id.to_string();
                    let lvl_str = li.level.to_string();
                    ppr.create_element("w:numPr").write_inner_content(|num| {
                        num.create_element("w:ilvl")
                            .with_attribute(("w:val", lvl_str.as_str()))
                            .write_empty()?;
                        num.create_element("w:numId")
                            .with_attribute(("w:val", id_str.as_str()))
                            .write_empty()?;
                        Ok(())
                    })?;
                }
                // Emit tab stops for numbered equations (right-aligned at page width)
                // or for recovered grid/multi-column layouts
                if has_eq_number {
                    ppr.create_element("w:tabs").write_inner_content(|tabs| {
                        tabs.create_element("w:tab")
                            .with_attribute(("w:val", "right"))
                            .with_attribute((
                                "w:pos",
                                parts.content_width_twips.to_string().as_str(),
                            ))
                            .write_empty()?;
                        Ok(())
                    })?;
                } else if has_tab_stops {
                    ppr.create_element("w:tabs").write_inner_content(|tabs| {
                        for &pos in &para.tab_stops {
                            let pos_str = pos.to_string();
                            tabs.create_element("w:tab")
                                .with_attribute(("w:val", "right"))
                                .with_attribute(("w:pos", pos_str.as_str()))
                                .write_empty()?;
                        }
                        Ok(())
                    })?;
                }
                // Emit indent: left indent (blockquote), hanging (bibliography), list, or suppress first-line
                if let Some(left) = para.left_indent {
                    let left_str = left.to_string();
                    ppr.create_element("w:ind")
                        .with_attribute(("w:left", left_str.as_str()))
                        .with_attribute(("w:firstLine", "0"))
                        .write_empty()?;
                } else if has_hanging {
                    // Bibliography hanging indent: 2em computed from body font size
                    let bib_indent = doc_style.body_size_half_pt * 10 * 2;
                    let bib_indent_str = bib_indent.to_string();
                    ppr.create_element("w:ind")
                        .with_attribute(("w:left", bib_indent_str.as_str()))
                        .with_attribute(("w:hanging", bib_indent_str.as_str()))
                        .with_attribute(("w:firstLine", "0"))
                        .write_empty()?;
                } else if has_list {
                    // List indent: left = 2em, hanging = 1em, computed from body font size
                    let list_left = doc_style.body_size_half_pt * 10 * 2;
                    let list_hanging = doc_style.body_size_half_pt * 10;
                    let list_left_str = list_left.to_string();
                    let list_hanging_str = list_hanging.to_string();
                    ppr.create_element("w:ind")
                        .with_attribute(("w:left", list_left_str.as_str()))
                        .with_attribute(("w:hanging", list_hanging_str.as_str()))
                        .write_empty()?;
                } else if suppress_indent || has_eq_number {
                    ppr.create_element("w:ind")
                        .with_attribute(("w:firstLine", "0"))
                        .write_empty()?;
                }
                // Emit spacing override
                if let Some(before) = para.spacing_before {
                    let before_str = before.to_string();
                    ppr.create_element("w:spacing")
                        .with_attribute(("w:before", before_str.as_str()))
                        .write_empty()?;
                }
                // Emit alignment
                if let Some(alignment) = &para.alignment {
                    ppr.create_element("w:jc")
                        .with_attribute(("w:val", alignment.as_ooxml_str()))
                        .write_empty()?;
                }
                // Emit section break (w:sectPr inside w:pPr)
                if let Some(section) = &para.section_break {
                    write_section_break(ppr, section, doc_style)?;
                }
                Ok(())
            })?;
        }
        for inline in &para.inlines {
            match inline {
                InlineElement::Text(run) => write_run(w, run, doc_style)?,
                InlineElement::FootnoteRef(id) => write_footnote_ref(w, *id, fn_format)?,
                InlineElement::Math {
                    omml_xml,
                    equation_number,
                } => {
                    write_math_inline(w, omml_xml)?;
                    if let Some(num) = equation_number {
                        write_equation_number(w, num)?;
                    }
                }
                InlineElement::Image(img) => {
                    let n = image_counter.get() + 1;
                    image_counter.set(n);
                    let rid = image_rel_id(n, parts);
                    write_image_inline(w, img, n, &rid)?;
                }
                InlineElement::Bookmark { id, name } => {
                    write_bookmark_start(w, *id, name)?;
                }
                InlineElement::BookmarkEnd { id } => {
                    write_bookmark_end(w, *id)?;
                }
                InlineElement::FieldRef {
                    bookmark_name,
                    display_text,
                } => {
                    write_field_ref(w, bookmark_name, display_text)?;
                }
                InlineElement::Hyperlink { url, runs } => {
                    write_hyperlink(w, url, runs, doc_style)?;
                }
                InlineElement::PageBreak => {
                    write_page_break(w)?;
                }
                InlineElement::FieldToc { max_depth } => {
                    write_toc_field(w, *max_depth)?;
                }
                InlineElement::Tab => {
                    write_tab(w)?;
                }
                InlineElement::Citation { keys, display_text } => {
                    let sdt_id = citation_id_counter.get();
                    citation_id_counter.set(sdt_id + 1);
                    write_citation_sdt(w, keys, display_text, sdt_id)?;
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn write_inline<W: Write>(
    writer: &mut Writer<W>,
    inline: &InlineElement,
    doc: &Document,
) -> io::Result<()> {
    match inline {
        InlineElement::Text(run) => write_run(writer, run, &doc.style),
        InlineElement::Math {
            omml_xml,
            equation_number: _,
        } => write_math_inline(writer, omml_xml),
        _ => Ok(()),
    }
}

/// Write a tab character element (`<w:r><w:tab/></w:r>`).
fn write_tab<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:tab").write_empty()?;
        Ok(())
    })?;
    Ok(())
}

fn write_run<W: Write>(
    writer: &mut Writer<W>,
    run: &crate::document::Run,
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        let has_font_override = run.font_ascii.is_some() || run.font_east_asia.is_some();
        let has_rpr = run.bold
            || run.italic
            || run.superscript
            || run.subscript
            || run.monospace
            || run.underline
            || run.strikethrough
            || run.highlight_color.is_some()
            || run.smallcaps
            || run.color.is_some()
            || has_font_override
            || run.size_half_pt.is_some();
        if has_rpr {
            w.create_element("w:rPr").write_inner_content(|rpr| {
                if run.monospace {
                    rpr.create_element("w:rFonts")
                        .with_attribute(("w:ascii", doc_style.code_font.as_str()))
                        .with_attribute(("w:hAnsi", doc_style.code_font.as_str()))
                        .with_attribute(("w:eastAsia", doc_style.code_font.as_str()))
                        .write_empty()?;
                } else if has_font_override {
                    let ascii = run
                        .font_ascii
                        .as_deref()
                        .unwrap_or(&doc_style.body_font_ascii);
                    let east_asia = run
                        .font_east_asia
                        .as_deref()
                        .unwrap_or(&doc_style.body_font_east_asia);
                    rpr.create_element("w:rFonts")
                        .with_attribute(("w:ascii", ascii))
                        .with_attribute(("w:hAnsi", ascii))
                        .with_attribute(("w:eastAsia", east_asia))
                        .write_empty()?;
                }
                if run.bold {
                    rpr.create_element("w:b").write_empty()?;
                }
                if run.italic {
                    rpr.create_element("w:i").write_empty()?;
                }
                if run.smallcaps {
                    rpr.create_element("w:smallCaps").write_empty()?;
                }
                if let Some(color) = &run.color {
                    rpr.create_element("w:color")
                        .with_attribute(("w:val", color.as_str()))
                        .write_empty()?;
                }
                if let Some(size) = run.size_half_pt {
                    let size_str = size.to_string();
                    rpr.create_element("w:sz")
                        .with_attribute(("w:val", size_str.as_str()))
                        .write_empty()?;
                    rpr.create_element("w:szCs")
                        .with_attribute(("w:val", size_str.as_str()))
                        .write_empty()?;
                }
                if run.strikethrough {
                    rpr.create_element("w:strike").write_empty()?;
                }
                if run.underline {
                    rpr.create_element("w:u")
                        .with_attribute(("w:val", "single"))
                        .write_empty()?;
                }
                if let Some(hl_color) = &run.highlight_color {
                    rpr.create_element("w:highlight")
                        .with_attribute(("w:val", hl_color.as_str()))
                        .write_empty()?;
                }
                if run.superscript {
                    rpr.create_element("w:vertAlign")
                        .with_attribute(("w:val", "superscript"))
                        .write_empty()?;
                }
                if run.subscript {
                    rpr.create_element("w:vertAlign")
                        .with_attribute(("w:val", "subscript"))
                        .write_empty()?;
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

#[allow(clippy::too_many_lines)]
fn write_table<W: Write>(
    writer: &mut Writer<W>,
    table: &Table,
    fn_format: &crate::document::FootnoteFormat,
    doc_style: &crate::document::DocumentStyle,
    parts: DocParts,
    image_counter: &std::cell::Cell<usize>,
    citation_id_counter: &std::cell::Cell<u32>,
) -> io::Result<()> {
    // Determine number of columns from the first row for equal-width distribution
    let num_cols = table
        .rows
        .first()
        .map_or(1, |r| r.cells.iter().map(|c| c.colspan).sum::<u32>());

    writer.create_element("w:tbl").write_inner_content(|w| {
        // Table properties with borders
        let tbl_width = table.width_pct.unwrap_or(5000).to_string();
        let border_sz = table.border_size.unwrap_or(4).to_string();
        w.create_element("w:tblPr").write_inner_content(|tpr| {
            tpr.create_element("w:tblW")
                .with_attribute(("w:w", tbl_width.as_str()))
                .with_attribute(("w:type", "pct"))
                .write_empty()?;
            tpr.create_element("w:tblBorders")
                .write_inner_content(|bdr| {
                    for side in [
                        "w:top",
                        "w:left",
                        "w:bottom",
                        "w:right",
                        "w:insideH",
                        "w:insideV",
                    ] {
                        bdr.create_element(side)
                            .with_attribute(("w:val", "single"))
                            .with_attribute(("w:sz", border_sz.as_str()))
                            .with_attribute(("w:space", "0"))
                            .write_empty()?;
                    }
                    Ok(())
                })?;
            Ok(())
        })?;
        // Table rows
        for row in &table.rows {
            w.create_element("w:tr").write_inner_content(|tr_w| {
                for cell in &row.cells {
                    tr_w.create_element("w:tc").write_inner_content(|tc_w| {
                        // Emit cell properties (width, merges)
                        let has_colspan = cell.colspan > 1;
                        let has_vmerge = cell.vmerge != crate::document::VMerge::None;
                        // Always emit tcPr to set cell width
                        tc_w.create_element("w:tcPr").write_inner_content(|tcpr| {
                            // Cell width: use explicit value or equal distribution
                            let cell_width = cell
                                .width_pct
                                .unwrap_or_else(|| (5000 / num_cols) * cell.colspan);
                            let width_str = cell_width.to_string();
                            tcpr.create_element("w:tcW")
                                .with_attribute(("w:w", width_str.as_str()))
                                .with_attribute(("w:type", "pct"))
                                .write_empty()?;
                            if has_colspan {
                                let span_str = cell.colspan.to_string();
                                tcpr.create_element("w:gridSpan")
                                    .with_attribute(("w:val", span_str.as_str()))
                                    .write_empty()?;
                            }
                            if has_vmerge {
                                match &cell.vmerge {
                                    crate::document::VMerge::Restart => {
                                        tcpr.create_element("w:vMerge")
                                            .with_attribute(("w:val", "restart"))
                                            .write_empty()?;
                                    }
                                    crate::document::VMerge::Continue => {
                                        tcpr.create_element("w:vMerge").write_empty()?;
                                    }
                                    crate::document::VMerge::None => {}
                                }
                            }
                            Ok(())
                        })?;
                        if !cell.content.is_empty() {
                            // Cell has structured content (paragraphs + nested tables)
                            let mut has_trailing_para = false;
                            for item in &cell.content {
                                match item {
                                    crate::document::CellContent::Paragraph(para) => {
                                        write_paragraph(
                                            tc_w,
                                            para,
                                            fn_format,
                                            doc_style,
                                            parts,
                                            image_counter,
                                            citation_id_counter,
                                        )?;
                                        has_trailing_para = true;
                                    }
                                    crate::document::CellContent::Table(nested_tbl) => {
                                        write_table(
                                            tc_w,
                                            nested_tbl,
                                            fn_format,
                                            doc_style,
                                            parts,
                                            image_counter,
                                            citation_id_counter,
                                        )?;
                                        has_trailing_para = false;
                                    }
                                }
                            }
                            // OOXML requires a trailing w:p after a nested w:tbl in a cell
                            if !has_trailing_para {
                                tc_w.create_element("w:p").write_empty()?;
                            }
                        } else if cell.paragraphs.is_empty() {
                            // OOXML requires at least one paragraph per cell
                            tc_w.create_element("w:p").write_empty()?;
                        } else {
                            for para in &cell.paragraphs {
                                write_paragraph(
                                    tc_w,
                                    para,
                                    fn_format,
                                    doc_style,
                                    parts,
                                    image_counter,
                                    citation_id_counter,
                                )?;
                            }
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn generate_numbering_xml(writer: &mut Writer<&mut Vec<u8>>, doc: &Document) -> io::Result<()> {
    writer
        .create_element("w:numbering")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            // Abstract numbering 1: ordered list (decimal)
            w.create_element("w:abstractNum")
                .with_attribute(("w:abstractNumId", "1"))
                .write_inner_content(|abs| {
                    abs.create_element("w:lvl")
                        .with_attribute(("w:ilvl", "0"))
                        .write_inner_content(|lvl| {
                            lvl.create_element("w:start")
                                .with_attribute(("w:val", "1"))
                                .write_empty()?;
                            lvl.create_element("w:numFmt")
                                .with_attribute(("w:val", "decimal"))
                                .write_empty()?;
                            lvl.create_element("w:lvlText")
                                .with_attribute(("w:val", "%1."))
                                .write_empty()?;
                            lvl.create_element("w:lvlJc")
                                .with_attribute(("w:val", "left"))
                                .write_empty()?;
                            Ok(())
                        })?;
                    Ok(())
                })?;
            // Abstract numbering 2: unordered list (bullet)
            w.create_element("w:abstractNum")
                .with_attribute(("w:abstractNumId", "2"))
                .write_inner_content(|abs| {
                    abs.create_element("w:lvl")
                        .with_attribute(("w:ilvl", "0"))
                        .write_inner_content(|lvl| {
                            lvl.create_element("w:start")
                                .with_attribute(("w:val", "1"))
                                .write_empty()?;
                            lvl.create_element("w:numFmt")
                                .with_attribute(("w:val", "bullet"))
                                .write_empty()?;
                            lvl.create_element("w:lvlText")
                                .with_attribute(("w:val", "\u{2022}"))
                                .write_empty()?;
                            lvl.create_element("w:lvlJc")
                                .with_attribute(("w:val", "left"))
                                .write_empty()?;
                            Ok(())
                        })?;
                    Ok(())
                })?;
            // Abstract numbering 3: Chinese five-level heading numbering
            // Available for opt-in use; not auto-linked to heading styles.
            w.create_element("w:abstractNum")
                .with_attribute(("w:abstractNumId", "3"))
                .write_inner_content(|abs| {
                    // Level 0: 一、二、三、 (chineseCountingThousand)
                    write_numbering_level(
                        abs,
                        "0",
                        "1",
                        "chineseCountingThousand",
                        "%1\u{3001}",
                        "left",
                    )?;
                    // Level 1: （一）（二）（三）
                    write_numbering_level(
                        abs,
                        "1",
                        "1",
                        "chineseCountingThousand",
                        "\u{ff08}%2\u{ff09}",
                        "left",
                    )?;
                    // Level 2: 1. 2. 3.
                    write_numbering_level(abs, "2", "1", "decimal", "%3.", "left")?;
                    // Level 3: （1）（2）（3）
                    write_numbering_level(abs, "3", "1", "decimal", "\u{ff08}%4\u{ff09}", "left")?;
                    // Level 4: ① ② ③
                    write_numbering_level(
                        abs,
                        "4",
                        "1",
                        "decimalEnclosedCircleChinese",
                        "%5",
                        "left",
                    )?;
                    Ok(())
                })?;
            // Numbering instance 1 -> abstractNum 1 (ordered)
            w.create_element("w:num")
                .with_attribute(("w:numId", "1"))
                .write_inner_content(|num| {
                    num.create_element("w:abstractNumId")
                        .with_attribute(("w:val", "1"))
                        .write_empty()?;
                    Ok(())
                })?;
            // Numbering instance 2 -> abstractNum 2 (unordered)
            w.create_element("w:num")
                .with_attribute(("w:numId", "2"))
                .write_inner_content(|num| {
                    num.create_element("w:abstractNumId")
                        .with_attribute(("w:val", "2"))
                        .write_empty()?;
                    Ok(())
                })?;
            // Numbering instance 3 -> abstractNum 3 (Chinese headings, opt-in)
            w.create_element("w:num")
                .with_attribute(("w:numId", "3"))
                .write_inner_content(|num| {
                    num.create_element("w:abstractNumId")
                        .with_attribute(("w:val", "3"))
                        .write_empty()?;
                    Ok(())
                })?;
            // Dynamic numbering instances for each top-level list
            for &(num_id, abstract_num_id) in &doc.list_num_instances {
                let num_id_str = num_id.to_string();
                let abs_id_str = abstract_num_id.to_string();
                w.create_element("w:num")
                    .with_attribute(("w:numId", num_id_str.as_str()))
                    .write_inner_content(|num| {
                        num.create_element("w:abstractNumId")
                            .with_attribute(("w:val", abs_id_str.as_str()))
                            .write_empty()?;
                        Ok(())
                    })?;
            }
            Ok(())
        })?;
    Ok(())
}

/// Helper to write a single numbering level definition.
fn write_numbering_level(
    writer: &mut Writer<&mut Vec<u8>>,
    ilvl: &str,
    start: &str,
    num_fmt: &str,
    lvl_text: &str,
    jc: &str,
) -> io::Result<()> {
    writer
        .create_element("w:lvl")
        .with_attribute(("w:ilvl", ilvl))
        .write_inner_content(|lvl| {
            lvl.create_element("w:start")
                .with_attribute(("w:val", start))
                .write_empty()?;
            lvl.create_element("w:numFmt")
                .with_attribute(("w:val", num_fmt))
                .write_empty()?;
            lvl.create_element("w:lvlText")
                .with_attribute(("w:val", lvl_text))
                .write_empty()?;
            lvl.create_element("w:lvlJc")
                .with_attribute(("w:val", jc))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn write_math_inline<W: Write>(writer: &mut Writer<W>, omml_xml: &str) -> io::Result<()> {
    // Write the pre-serialized OMML XML directly into the stream
    writer.get_mut().write_all(omml_xml.as_bytes())?;
    Ok(())
}

/// Write a right-aligned equation number after an OMML block equation.
///
/// This uses a right-aligned tab stop to position the number at the right margin,
/// mimicking the standard Chinese journal equation numbering style.
fn write_equation_number<W: Write>(writer: &mut Writer<W>, number: &str) -> io::Result<()> {
    // Emit a run with a tab character followed by the equation number
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:tab").write_empty()?;
        Ok(())
    })?;
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:t")
            .with_attribute(("xml:space", "preserve"))
            .write_text_content(BytesText::new(number))?;
        Ok(())
    })?;
    Ok(())
}

/// Write a `wp:inline` drawing element for an embedded image.
#[allow(clippy::too_many_lines)]
fn write_image_inline<W: Write>(
    writer: &mut Writer<W>,
    img: &ImageData,
    n: usize,
    rid: &str,
) -> io::Result<()> {
    let cx = img.width_emu.to_string();
    let cy = img.height_emu.to_string();
    let id_str = n.to_string();
    let ext = match img.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
    };
    let name = format!("Image{n}");
    let filename = format!("image{n}.{ext}");

    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:drawing").write_inner_content(|dw| {
            dw.create_element("wp:inline")
                .with_attribute(("distT", "0"))
                .with_attribute(("distB", "0"))
                .with_attribute(("distL", "0"))
                .with_attribute(("distR", "0"))
                .write_inner_content(|inl| {
                    inl.create_element("wp:extent")
                        .with_attribute(("cx", cx.as_str()))
                        .with_attribute(("cy", cy.as_str()))
                        .write_empty()?;
                    inl.create_element("wp:docPr")
                        .with_attribute(("id", id_str.as_str()))
                        .with_attribute(("name", name.as_str()))
                        .write_empty()?;
                    inl.create_element("a:graphic")
                        .with_attribute(("xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main"))
                        .write_inner_content(|gr| {
                            gr.create_element("a:graphicData")
                                .with_attribute(("uri", "http://schemas.openxmlformats.org/drawingml/2006/picture"))
                                .write_inner_content(|gd| {
                                    gd.create_element("pic:pic")
                                        .with_attribute(("xmlns:pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"))
                                        .write_inner_content(|pic| {
                                            pic.create_element("pic:nvPicPr").write_inner_content(|nv| {
                                                nv.create_element("pic:cNvPr")
                                                    .with_attribute(("id", id_str.as_str()))
                                                    .with_attribute(("name", filename.as_str()))
                                                    .write_empty()?;
                                                nv.create_element("pic:cNvPicPr").write_empty()?;
                                                Ok(())
                                            })?;
                                            pic.create_element("pic:blipFill").write_inner_content(|bf| {
                                                bf.create_element("a:blip")
                                                    .with_attribute(("r:embed", rid))
                                                    .write_empty()?;
                                                bf.create_element("a:stretch").write_inner_content(|st| {
                                                    st.create_element("a:fillRect").write_empty()?;
                                                    Ok(())
                                                })?;
                                                Ok(())
                                            })?;
                                            pic.create_element("pic:spPr").write_inner_content(|sp| {
                                                sp.create_element("a:xfrm").write_inner_content(|xf| {
                                                    xf.create_element("a:off")
                                                        .with_attribute(("x", "0"))
                                                        .with_attribute(("y", "0"))
                                                        .write_empty()?;
                                                    xf.create_element("a:ext")
                                                        .with_attribute(("cx", cx.as_str()))
                                                        .with_attribute(("cy", cy.as_str()))
                                                        .write_empty()?;
                                                    Ok(())
                                                })?;
                                                sp.create_element("a:prstGeom")
                                                    .with_attribute(("prst", "rect"))
                                                    .write_inner_content(|pg| {
                                                        pg.create_element("a:avLst").write_empty()?;
                                                        Ok(())
                                                    })?;
                                                Ok(())
                                            })?;
                                            Ok(())
                                        })?;
                                    Ok(())
                                })?;
                            Ok(())
                        })?;
                    Ok(())
                })?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

fn truncate_bookmark_name(name: &str) -> &str {
    if name.len() <= 40 {
        return name;
    }
    let mut end = 40;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

fn write_bookmark_start<W: Write>(writer: &mut Writer<W>, id: u32, name: &str) -> io::Result<()> {
    let id_str = id.to_string();
    let name = truncate_bookmark_name(name);
    writer
        .create_element("w:bookmarkStart")
        .with_attribute(("w:id", id_str.as_str()))
        .with_attribute(("w:name", name))
        .write_empty()?;
    Ok(())
}

fn write_bookmark_end<W: Write>(writer: &mut Writer<W>, id: u32) -> io::Result<()> {
    let id_str = id.to_string();
    writer
        .create_element("w:bookmarkEnd")
        .with_attribute(("w:id", id_str.as_str()))
        .write_empty()?;
    Ok(())
}

/// Write a `<w:r><w:fldChar w:fldCharType="..."/></w:r>` element.
fn write_fld_char<W: Write>(writer: &mut Writer<W>, char_type: &str) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:fldChar")
            .with_attribute(("w:fldCharType", char_type))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
}

fn write_field_ref<W: Write>(
    writer: &mut Writer<W>,
    bookmark_name: &str,
    display_text: &str,
) -> io::Result<()> {
    // fldChar begin
    write_fld_char(writer, "begin")?;
    // instrText with REF field code
    let bookmark_name = truncate_bookmark_name(bookmark_name);
    let instr = format!(" REF {bookmark_name} \\h ");
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:instrText")
            .with_attribute(("xml:space", "preserve"))
            .write_text_content(BytesText::new(&instr))?;
        Ok(())
    })?;
    // fldChar separate
    write_fld_char(writer, "separate")?;
    // Display text (fallback)
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:t")
            .with_attribute(("xml:space", "preserve"))
            .write_text_content(BytesText::new(display_text))?;
        Ok(())
    })?;
    // fldChar end
    write_fld_char(writer, "end")?;
    Ok(())
}

/// Write a Word citation as a Structured Document Tag (SDT) with a CITATION field code.
///
/// Produces:
/// ```xml
/// <w:sdt>
///   <w:sdtPr><w:id w:val="..."/><w:citation/></w:sdtPr>
///   <w:sdtContent>
///     <w:r><w:fldChar w:fldCharType="begin"/></w:r>
///     <w:r><w:instrText xml:space="preserve"> CITATION key1 \l 1033 [\m key2 ...] </w:instrText></w:r>
///     <w:r><w:fldChar w:fldCharType="separate"/></w:r>
///     <w:r><w:rPr><w:noProof/></w:rPr><w:t xml:space="preserve">(display text)</w:t></w:r>
///     <w:r><w:fldChar w:fldCharType="end"/></w:r>
///   </w:sdtContent>
/// </w:sdt>
/// ```
fn write_citation_sdt<W: Write>(
    writer: &mut Writer<W>,
    keys: &[String],
    display_text: &str,
    sdt_id: u32,
) -> io::Result<()> {
    writer
        .create_element("w:sdt")
        .write_inner_content(|sdt| {
            // SDT properties
            let id_str = sdt_id.to_string();
            sdt.create_element("w:sdtPr").write_inner_content(|pr| {
                pr.create_element("w:id")
                    .with_attribute(("w:val", id_str.as_str()))
                    .write_empty()?;
                pr.create_element("w:citation").write_empty()?;
                Ok(())
            })?;
            // SDT content: field code sequence
            sdt.create_element("w:sdtContent")
                .write_inner_content(|content| {
                    // fldChar begin
                    write_fld_char(content, "begin")?;
                    // instrText: CITATION key1 \l 1033 [\m key2 \m key3 ...]
                    let mut instr = String::new();
                    instr.push_str(" CITATION ");
                    if let Some(first) = keys.first() {
                        instr.push_str(first);
                    }
                    instr.push_str(r" \l 1033");
                    for key in keys.iter().skip(1) {
                        instr.push_str(r" \m ");
                        instr.push_str(key);
                    }
                    instr.push(' ');
                    content.create_element("w:r").write_inner_content(|w| {
                        w.create_element("w:instrText")
                            .with_attribute(("xml:space", "preserve"))
                            .write_text_content(BytesText::new(&instr))?;
                        Ok(())
                    })?;
                    // fldChar separate
                    write_fld_char(content, "separate")?;
                    // Display text with noProof
                    content.create_element("w:r").write_inner_content(|w| {
                        w.create_element("w:rPr").write_inner_content(|rpr| {
                            rpr.create_element("w:noProof").write_empty()?;
                            Ok(())
                        })?;
                        w.create_element("w:t")
                            .with_attribute(("xml:space", "preserve"))
                            .write_text_content(BytesText::new(display_text))?;
                        Ok(())
                    })?;
                    // fldChar end
                    write_fld_char(content, "end")?;
                    Ok(())
                })?;
            Ok(())
        })?;
    Ok(())
}

/// Write a block-level bibliography SDT wrapping bibliography paragraphs with a BIBLIOGRAPHY
/// field code.
///
/// Produces:
/// ```xml
/// <w:sdt>
///   <w:sdtPr><w:id w:val="..."/><w:bibliography/></w:sdtPr>
///   <w:sdtContent>
///     <w:p>
///       <w:pPr><w:pStyle w:val="Bibliography"/></w:pPr>
///       <w:r><w:fldChar w:fldCharType="begin"/></w:r>
///       <w:r><w:instrText xml:space="preserve"> BIBLIOGRAPHY </w:instrText></w:r>
///       <w:r><w:fldChar w:fldCharType="separate"/></w:r>
///     </w:p>
///     <!-- cached bibliography paragraphs -->
///     <w:p>
///       <w:r><w:fldChar w:fldCharType="end"/></w:r>
///     </w:p>
///   </w:sdtContent>
/// </w:sdt>
/// ```
fn write_bibliography_sdt<W: Write>(
    writer: &mut Writer<W>,
    paragraphs: &[crate::document::Paragraph],
    fn_format: &FootnoteFormat,
    doc_style: &crate::document::DocumentStyle,
    parts: DocParts,
    image_counter: &std::cell::Cell<usize>,
    citation_id_counter: &std::cell::Cell<u32>,
) -> io::Result<()> {
    writer
        .create_element("w:sdt")
        .write_inner_content(|sdt| {
            // SDT properties with bibliography marker
            sdt.create_element("w:sdtPr").write_inner_content(|pr| {
                let sdt_id = citation_id_counter.get();
                citation_id_counter.set(sdt_id + 1);
                let id_str = sdt_id.to_string();
                pr.create_element("w:id")
                    .with_attribute(("w:val", id_str.as_str()))
                    .write_empty()?;
                pr.create_element("w:docPartObj")
                    .write_inner_content(|dpo| {
                        dpo.create_element("w:docPartGallery")
                            .with_attribute(("w:val", "Bibliographies"))
                            .write_empty()?;
                        dpo.create_element("w:docPartUnique")
                            .write_empty()?;
                        Ok(())
                    })?;
                pr.create_element("w:bibliography").write_empty()?;
                Ok(())
            })?;
            // SDT content
            sdt.create_element("w:sdtContent")
                .write_inner_content(|content| {
                    // Opening paragraph with Bibliography style + field begin + instrText + field separate
                    content.create_element("w:p").write_inner_content(|pw| {
                        pw.create_element("w:pPr").write_inner_content(|ppr| {
                            ppr.create_element("w:pStyle")
                                .with_attribute(("w:val", "Bibliography"))
                                .write_empty()?;
                            Ok(())
                        })?;
                        // fldChar begin
                        write_fld_char(pw, "begin")?;
                        // instrText
                        pw.create_element("w:r").write_inner_content(|w| {
                            w.create_element("w:instrText")
                                .with_attribute(("xml:space", "preserve"))
                                .write_text_content(BytesText::new(" BIBLIOGRAPHY "))?;
                            Ok(())
                        })?;
                        // fldChar separate
                        write_fld_char(pw, "separate")?;
                        Ok(())
                    })?;
                    // Cached bibliography paragraphs
                    for para in paragraphs {
                        write_paragraph(
                            content,
                            para,
                            fn_format,
                            doc_style,
                            parts,
                            image_counter,
                            citation_id_counter,
                        )?;
                    }
                    // Closing paragraph with field end
                    content.create_element("w:p").write_inner_content(|pw| {
                        write_fld_char(pw, "end")?;
                        Ok(())
                    })?;
                    Ok(())
                })?;
            Ok(())
        })?;
    Ok(())
}

fn write_hyperlink<W: Write>(
    writer: &mut Writer<W>,
    url: &str,
    runs: &[crate::document::Run],
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    // Use w:fldSimple with HYPERLINK field code to avoid relationship management
    let instr = format!("HYPERLINK &quot;{url}&quot;");
    writer
        .create_element("w:fldSimple")
        .with_attribute(("w:instr", instr.as_str()))
        .write_inner_content(|w| {
            for run in runs {
                w.create_element("w:r").write_inner_content(|rw| {
                    // Apply hyperlink styling: colored + underlined
                    rw.create_element("w:rPr").write_inner_content(|rpr| {
                        rpr.create_element("w:color")
                            .with_attribute(("w:val", doc_style.hyperlink_color.as_str()))
                            .write_empty()?;
                        rpr.create_element("w:u")
                            .with_attribute(("w:val", "single"))
                            .write_empty()?;
                        if run.bold {
                            rpr.create_element("w:b").write_empty()?;
                        }
                        if run.italic {
                            rpr.create_element("w:i").write_empty()?;
                        }
                        Ok(())
                    })?;
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

fn write_page_break<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:br")
            .with_attribute(("w:type", "page"))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
}

fn write_toc_field<W: Write>(writer: &mut Writer<W>, max_depth: u8) -> io::Result<()> {
    let instr = format!(r#" TOC \o "1-{max_depth}" \h \z \u "#);
    // fldChar begin
    write_fld_char(writer, "begin")?;
    // instrText
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:instrText")
            .with_attribute(("xml:space", "preserve"))
            .write_text_content(quick_xml::events::BytesText::new(&instr))?;
        Ok(())
    })?;
    // fldChar separate
    write_fld_char(writer, "separate")?;
    // Placeholder text
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:t")
            .write_text_content(quick_xml::events::BytesText::new(
                "Right-click and update field to see table of contents.",
            ))?;
        Ok(())
    })?;
    // fldChar end
    write_fld_char(writer, "end")?;
    Ok(())
}

fn write_footnote_ref<W: Write>(
    writer: &mut Writer<W>,
    id: u32,
    footnote_format: &crate::document::FootnoteFormat,
) -> io::Result<()> {
    let id_str = id.to_string();
    let use_custom = *footnote_format == crate::document::FootnoteFormat::CircledNumber;

    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:rPr").write_inner_content(|rpr| {
            rpr.create_element("w:rStyle")
                .with_attribute(("w:val", "FootnoteReference"))
                .write_empty()?;
            Ok(())
        })?;
        if use_custom {
            w.create_element("w:footnoteReference")
                .with_attribute(("w:customMarkFollows", "1"))
                .with_attribute(("w:id", id_str.as_str()))
                .write_empty()?;
        } else {
            w.create_element("w:footnoteReference")
                .with_attribute(("w:id", id_str.as_str()))
                .write_empty()?;
        }
        Ok(())
    })?;

    if use_custom {
        let seq_num = id - 1; // id starts at 2, so fn 1 = id 2
        let mark = circled_number_char(seq_num);
        writer.create_element("w:r").write_inner_content(|w| {
            w.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:rStyle")
                    .with_attribute(("w:val", "FootnoteReference"))
                    .write_empty()?;
                Ok(())
            })?;
            w.create_element("w:t")
                .with_attribute(("xml:space", "preserve"))
                .write_text_content(BytesText::new(&mark))?;
            Ok(())
        })?;
    }

    Ok(())
}

fn circled_number_char(n: u32) -> String {
    let c = match n {
        1..=20 => char::from_u32(0x2460 + n - 1),   // ① to ⑳
        21..=35 => char::from_u32(0x3251 + n - 21), // ㉑ to ㉟
        36..=50 => char::from_u32(0x32B1 + n - 36), // ㊱ to ㊿
        _ => None,
    };
    c.map_or_else(|| n.to_string(), |c| c.to_string())
}

fn generate_footnotes_xml(writer: &mut Writer<&mut Vec<u8>>, doc: &Document) -> io::Result<()> {
    writer
        .create_element("w:footnotes")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ))
        .write_inner_content(|w| {
            // Separator footnotes (id 0 and 1 are reserved by OOXML spec)
            write_separator_footnote(w, "0", "separator")?;
            write_separator_footnote(w, "1", "continuationSeparator")?;

            // Actual footnotes
            for footnote in &doc.footnotes {
                let id_str = footnote.id.to_string();
                w.create_element("w:footnote")
                    .with_attribute(("w:id", id_str.as_str()))
                    .write_inner_content(|fn_w| {
                        fn_w.create_element("w:p").write_inner_content(|p_w| {
                            // Paragraph properties with footnote text style
                            p_w.create_element("w:pPr").write_inner_content(|ppr| {
                                ppr.create_element("w:pStyle")
                                    .with_attribute(("w:val", "FootnoteText"))
                                    .write_empty()?;
                                Ok(())
                            })?;
                            let use_custom = doc.style.footnote_format
                                == crate::document::FootnoteFormat::CircledNumber;
                            if use_custom {
                                let seq = footnote.id - 1;
                                let mark = circled_number_char(seq);
                                p_w.create_element("w:r").write_inner_content(|r_w| {
                                    r_w.create_element("w:rPr").write_inner_content(|rpr| {
                                        rpr.create_element("w:rStyle")
                                            .with_attribute(("w:val", "FootnoteReference"))
                                            .write_empty()?;
                                        Ok(())
                                    })?;
                                    r_w.create_element("w:t")
                                        .write_text_content(BytesText::new(&mark))?;
                                    Ok(())
                                })?;
                            } else {
                                p_w.create_element("w:r").write_inner_content(|r_w| {
                                    r_w.create_element("w:rPr").write_inner_content(|rpr| {
                                        rpr.create_element("w:rStyle")
                                            .with_attribute(("w:val", "FootnoteReference"))
                                            .write_empty()?;
                                        Ok(())
                                    })?;
                                    r_w.create_element("w:footnoteRef").write_empty()?;
                                    Ok(())
                                })?;
                            }
                            // Space after reference mark
                            p_w.create_element("w:r").write_inner_content(|r_w| {
                                r_w.create_element("w:t")
                                    .with_attribute(("xml:space", "preserve"))
                                    .write_text_content(BytesText::new(" "))?;
                                Ok(())
                            })?;
                            // Content inlines
                            for inline in &footnote.content {
                                write_inline(p_w, inline, doc)?;
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })?;
            }
            Ok(())
        })?;
    Ok(())
}

/// Write a `w:sectPr` element inside a paragraph's `w:pPr` for a section break.
fn write_section_break<W: Write>(
    writer: &mut Writer<W>,
    section: &SectionBreak,
    style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    writer.create_element("w:sectPr").write_inner_content(|w| {
        let break_val = match section.break_type {
            SectionBreakType::NextPage => "nextPage",
            SectionBreakType::Continuous => "continuous",
            SectionBreakType::EvenPage => "evenPage",
            SectionBreakType::OddPage => "oddPage",
        };
        w.create_element("w:type")
            .with_attribute(("w:val", break_val))
            .write_empty()?;
        if let Some(ps) = &section.page_settings {
            write_section_page_settings(w, ps, style)?;
        } else {
            // Use default page settings
            write_section_page_settings(w, &crate::document::PageSettings::default(), style)?;
        }
        Ok(())
    })?;
    Ok(())
}

fn generate_header_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    content: &crate::document::HeaderFooter,
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    generate_header_footer_part(writer, "w:hdr", content, doc_style)
}

fn generate_footer_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    content: &crate::document::HeaderFooter,
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    generate_header_footer_part(writer, "w:ftr", content, doc_style)
}

/// Generate a footer XML part containing a PAGE field code for automatic page numbering.
///
/// Produces a centered paragraph with `fldChar begin / instrText PAGE / fldChar separate /
/// fallback text / fldChar end`.
fn generate_page_number_footer_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    _fmt: &PageNumberFormat,
) -> io::Result<()> {
    writer
        .create_element("w:ftr")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ))
        .write_inner_content(|w| {
            w.create_element("w:p").write_inner_content(|pw| {
                // Center-aligned paragraph
                pw.create_element("w:pPr").write_inner_content(|ppr| {
                    ppr.create_element("w:jc")
                        .with_attribute(("w:val", "center"))
                        .write_empty()?;
                    Ok(())
                })?;
                // fldChar begin
                write_fld_char(pw, "begin")?;
                // instrText with PAGE field code
                pw.create_element("w:r").write_inner_content(|rw| {
                    rw.create_element("w:instrText")
                        .with_attribute(("xml:space", "preserve"))
                        .write_text_content(BytesText::new(" PAGE "))?;
                    Ok(())
                })?;
                // fldChar separate
                write_fld_char(pw, "separate")?;
                // Fallback display text
                pw.create_element("w:r").write_inner_content(|rw| {
                    rw.create_element("w:t")
                        .write_text_content(BytesText::new("1"))?;
                    Ok(())
                })?;
                // fldChar end
                write_fld_char(pw, "end")?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

/// Convert a `PageNumberFormat` to the OOXML `w:pgNumType/@w:fmt` value.
fn page_number_format_val(fmt: &PageNumberFormat) -> &'static str {
    match fmt {
        PageNumberFormat::Decimal => "decimal",
        PageNumberFormat::LowerRoman => "lowerRoman",
        PageNumberFormat::UpperRoman => "upperRoman",
        PageNumberFormat::LowerLetter => "lowerLetter",
        PageNumberFormat::UpperLetter => "upperLetter",
    }
}

fn generate_header_footer_part(
    writer: &mut Writer<&mut Vec<u8>>,
    root_tag: &str,
    content: &crate::document::HeaderFooter,
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    writer
        .create_element(root_tag)
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .with_attribute((
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ))
        .write_inner_content(|w| {
            for para in &content.paragraphs {
                write_header_footer_paragraph(w, para, doc_style)?;
            }
            Ok(())
        })?;
    Ok(())
}

/// Write a simplified paragraph for headers/footers (text runs only, with alignment).
fn write_header_footer_paragraph<W: Write>(
    writer: &mut Writer<W>,
    para: &crate::document::Paragraph,
    doc_style: &crate::document::DocumentStyle,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        let has_alignment = para.alignment.is_some();
        if has_alignment {
            w.create_element("w:pPr").write_inner_content(|ppr| {
                if let Some(alignment) = &para.alignment {
                    ppr.create_element("w:jc")
                        .with_attribute(("w:val", alignment.as_ooxml_str()))
                        .write_empty()?;
                }
                Ok(())
            })?;
        }
        for inline in &para.inlines {
            if let InlineElement::Text(run) = inline {
                write_run(w, run, doc_style)?;
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn tag_to_guid(tag: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tag.hash(&mut hasher);
    let h = hasher.finish();
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}",
        (h >> 32) as u32,
        ((h >> 16) & 0xFFFF) as u16,
        (h & 0xFFFF) as u16,
        ((h >> 48) & 0xFFFF) as u16,
        h & 0xFFFF_FFFF_FFFF
    )
}

/// Generate the `customXml/item1.xml` bibliography data source.
///
/// Produces:
/// ```xml
/// <b:Sources xmlns:b="..." xmlns="..." SelectedStyle="..." StyleName="APA">
///   <b:Source>
///     <b:Tag>key</b:Tag>
///     <b:SourceType>JournalArticle</b:SourceType>
///     <b:Author><b:Author><b:NameList><b:Person>...</b:Person></b:NameList></b:Author></b:Author>
///     <b:Title>...</b:Title>
///     ...
///   </b:Source>
/// </b:Sources>
/// ```
fn generate_custom_xml_sources(
    writer: &mut Writer<&mut Vec<u8>>,
    sources: &[crate::document::CitationSource],
) -> io::Result<()> {
    // APA is the default and most common academic citation style in Word.
    writer
        .create_element("b:Sources")
        .with_attribute((
            "xmlns:b",
            "http://schemas.openxmlformats.org/officeDocument/2006/bibliography",
        ))
        .with_attribute((
            "xmlns",
            "http://schemas.openxmlformats.org/officeDocument/2006/bibliography",
        ))
        .with_attribute(("SelectedStyle", r"\APASixthEditionOfficeOnline.xsl"))
        .with_attribute(("StyleName", "APA"))
        .write_inner_content(|w| {
            for src in sources {
                w.create_element("b:Source").write_inner_content(|s| {
                    s.create_element("b:Tag")
                        .write_text_content(BytesText::new(&src.tag))?;
                    s.create_element("b:SourceType")
                        .write_text_content(BytesText::new(src.source_type.as_ooxml_str()))?;
                    s.create_element("b:Guid")
                        .write_text_content(BytesText::new(&tag_to_guid(&src.tag)))?;
                    if !src.authors.is_empty() {
                        write_bibliography_authors(s, &src.authors)?;
                    }
                    write_optional_bib_field(s, "b:Title", src.title.as_deref())?;
                    write_optional_bib_field(s, "b:Year", src.year.as_deref())?;
                    write_optional_bib_field(s, "b:JournalName", src.journal_name.as_deref())?;
                    write_optional_bib_field(s, "b:Volume", src.volume.as_deref())?;
                    write_optional_bib_field(s, "b:Issue", src.issue.as_deref())?;
                    write_optional_bib_field(s, "b:Pages", src.pages.as_deref())?;
                    write_optional_bib_field(s, "b:DOI", src.doi.as_deref())?;
                    write_optional_bib_field(s, "b:URL", src.url.as_deref())?;
                    write_optional_bib_field(s, "b:Publisher", src.publisher.as_deref())?;
                    write_optional_bib_field(s, "b:City", src.city.as_deref())?;
                    write_optional_bib_field(s, "b:Edition", src.edition.as_deref())?;
                    write_optional_bib_field(s, "b:BookTitle", src.book_title.as_deref())?;
                    Ok(())
                })?;
            }
            Ok(())
        })?;
    Ok(())
}

/// Write the `b:Author > b:Author > b:NameList > b:Person` nesting for bibliography authors.
fn write_bibliography_authors(
    writer: &mut Writer<&mut Vec<u8>>,
    authors: &[crate::document::PersonName],
) -> io::Result<()> {
    writer
        .create_element("b:Author")
        .write_inner_content(|a_outer| {
            a_outer
                .create_element("b:Author")
                .write_inner_content(|a_inner| {
                    a_inner
                        .create_element("b:NameList")
                        .write_inner_content(|nl| {
                            for person in authors {
                                nl.create_element("b:Person").write_inner_content(|p| {
                                    p.create_element("b:Last")
                                        .write_text_content(BytesText::new(&person.last))?;
                                    write_optional_bib_field(p, "b:First", person.first.as_deref())?;
                                    write_optional_bib_field(p, "b:Middle", person.middle.as_deref())?;
                                    Ok(())
                                })?;
                            }
                            Ok(())
                        })?;
                    Ok(())
                })?;
            Ok(())
        })?;
    Ok(())
}

/// Write a bibliography field element only when the value is `Some`.
fn write_optional_bib_field(
    writer: &mut Writer<&mut Vec<u8>>,
    element: &str,
    value: Option<&str>,
) -> io::Result<()> {
    if let Some(v) = value {
        writer
            .create_element(element)
            .write_text_content(BytesText::new(v))?;
    }
    Ok(())
}

/// Generate the `customXml/itemProps1.xml` schema URI declaration.
fn generate_custom_xml_item_props(
    writer: &mut Writer<&mut Vec<u8>>,
    sources: &[crate::document::CitationSource],
) -> io::Result<()> {
    let guid = tag_to_guid(
        &sources.iter().map(|s| s.tag.as_str()).collect::<Vec<_>>().join(","),
    );
    writer
        .create_element("ds:datastoreItem")
        .with_attribute(("ds:itemID", guid.as_str()))
        .with_attribute((
            "xmlns:ds",
            "http://schemas.openxmlformats.org/officeDocument/2006/customXml",
        ))
        .write_inner_content(|w| {
            w.create_element("ds:schemaRefs").write_inner_content(|sr| {
                sr.create_element("ds:schemaRef")
                    .with_attribute((
                        "ds:uri",
                        "http://schemas.openxmlformats.org/officeDocument/2006/bibliography",
                    ))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

/// Generate the `customXml/_rels/item1.xml.rels` relationship file.
fn generate_custom_xml_rels(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps",
                ))
                .with_attribute(("Target", "itemProps1.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn generate_core_properties(writer: &mut Writer<&mut Vec<u8>>, doc: &Document) -> io::Result<()> {
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

fn write_separator_footnote<W: Write>(
    writer: &mut Writer<W>,
    id: &str,
    sep_type: &str,
) -> io::Result<()> {
    writer
        .create_element("w:footnote")
        .with_attribute(("w:type", sep_type))
        .with_attribute(("w:id", id))
        .write_inner_content(|w| {
            w.create_element("w:p").write_inner_content(|p_w| {
                p_w.create_element("w:r").write_inner_content(|r_w| {
                    r_w.create_element("w:separator").write_empty()?;
                    Ok(())
                })?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}
