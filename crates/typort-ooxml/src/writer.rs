use std::io::{self, Seek, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{BlockElement, Document, InlineElement, ParagraphStyle, Table};
use crate::styles;

/// Write a Document to a .docx file (ZIP archive) into the given writer.
///
/// # Errors
/// Returns an error if XML serialization or ZIP writing fails.
pub fn write_docx<W: Write + Seek>(
    doc: &Document,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let has_footnotes = !doc.footnotes.is_empty();
    let has_numbering = doc_has_lists(doc);

    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(&xml_part(|w| {
        generate_content_types(w, has_footnotes, has_numbering)
    })?)?;

    zip.start_file("_rels/.rels", options)?;
    zip.write_all(&xml_part(generate_rels)?)?;

    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(&xml_part(|w| {
        generate_document_rels(w, has_footnotes, has_numbering)
    })?)?;

    zip.start_file("word/styles.xml", options)?;
    zip.write_all(&xml_part(|w| styles::generate_styles(w, has_footnotes))?)?;

    zip.start_file("word/fontTable.xml", options)?;
    zip.write_all(&xml_part(styles::generate_font_table)?)?;

    zip.start_file("word/document.xml", options)?;
    zip.write_all(&xml_part(|w| generate_document_xml(w, doc))?)?;

    if has_footnotes {
        zip.start_file("word/footnotes.xml", options)?;
        zip.write_all(&xml_part(|w| generate_footnotes_xml(w, doc))?)?;
    }

    if has_numbering {
        zip.start_file("word/numbering.xml", options)?;
        zip.write_all(&xml_part(generate_numbering_xml)?)?;
    }

    // Always write docProps/core.xml with metadata
    zip.start_file("docProps/core.xml", options)?;
    zip.write_all(&xml_part(|w| generate_core_properties(w, doc))?)?;

    zip.finish()?;
    Ok(())
}

/// Check if the document contains any list items (paragraphs with `list_id` set).
fn doc_has_lists(doc: &Document) -> bool {
    doc.body.elements.iter().any(|el| match el {
        BlockElement::Paragraph(p) => p.list_id.is_some(),
        BlockElement::Table(_) => false,
    })
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
    has_footnotes: bool,
    has_numbering: bool,
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
            if has_footnotes {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/footnotes.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"))
                    .write_empty()?;
            }
            if has_numbering {
                w.create_element("Override")
                    .with_attribute(("PartName", "/word/numbering.xml"))
                    .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"))
                    .write_empty()?;
            }
            w.create_element("Override")
                .with_attribute(("PartName", "/docProps/core.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-package.core-properties+xml"))
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
            w.create_element("Relationship")
                .with_attribute(("Id", "rId2"))
                .with_attribute(("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"))
                .with_attribute(("Target", "docProps/core.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn generate_document_rels(
    writer: &mut Writer<&mut Vec<u8>>,
    has_footnotes: bool,
    has_numbering: bool,
) -> io::Result<()> {
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
            if has_footnotes {
                w.create_element("Relationship")
                    .with_attribute(("Id", "rId3"))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
                    ))
                    .with_attribute(("Target", "footnotes.xml"))
                    .write_empty()?;
            }
            if has_numbering {
                let num_id = if has_footnotes { "rId4" } else { "rId3" };
                w.create_element("Relationship")
                    .with_attribute(("Id", num_id))
                    .with_attribute((
                        "Type",
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
                    ))
                    .with_attribute(("Target", "numbering.xml"))
                    .write_empty()?;
            }
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
        .with_attribute((
            "xmlns:m",
            "http://schemas.openxmlformats.org/officeDocument/2006/math",
        ))
        .write_inner_content(|w| {
            w.create_element("w:body").write_inner_content(|body_w| {
                for element in &doc.body.elements {
                    match element {
                        BlockElement::Paragraph(para) => {
                            write_paragraph(body_w, para)?;
                        }
                        BlockElement::Table(table) => {
                            write_table(body_w, table)?;
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
        let has_style = para.style.is_some();
        let has_list = para.list_id.is_some();
        if has_style || has_list {
            w.create_element("w:pPr").write_inner_content(|ppr| {
                if let Some(style) = &para.style {
                    let style_id = match style {
                        ParagraphStyle::Heading(n) => format!("Heading{n}"),
                        ParagraphStyle::Normal => "Normal".to_string(),
                    };
                    ppr.create_element("w:pStyle")
                        .with_attribute(("w:val", style_id.as_str()))
                        .write_empty()?;
                }
                if let (Some(list_id), Some(list_level)) = (para.list_id, para.list_level) {
                    let id_str = list_id.to_string();
                    let lvl_str = list_level.to_string();
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
                Ok(())
            })?;
        }
        // Use the inlines list if it has content (supports footnote refs);
        // otherwise fall back to runs for backward compat.
        if para.inlines.is_empty() {
            for run in &para.runs {
                write_run(w, run)?;
            }
        } else {
            for inline in &para.inlines {
                match inline {
                    InlineElement::Text(run) => write_run(w, run)?,
                    InlineElement::FootnoteRef(id) => write_footnote_ref(w, *id)?,
                    InlineElement::Math { omml_xml } => write_math_inline(w, omml_xml)?,
                }
            }
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

fn write_table<W: Write>(writer: &mut Writer<W>, table: &Table) -> io::Result<()> {
    writer.create_element("w:tbl").write_inner_content(|w| {
        // Table properties with borders
        w.create_element("w:tblPr").write_inner_content(|tpr| {
            tpr.create_element("w:tblW")
                .with_attribute(("w:w", "0"))
                .with_attribute(("w:type", "auto"))
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
                            .with_attribute(("w:sz", "4"))
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
                        if cell.paragraphs.is_empty() {
                            // OOXML requires at least one paragraph per cell
                            tc_w.create_element("w:p").write_empty()?;
                        } else {
                            for para in &cell.paragraphs {
                                write_paragraph(tc_w, para)?;
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

fn generate_numbering_xml(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
            Ok(())
        })?;
    Ok(())
}

fn write_math_inline<W: Write>(writer: &mut Writer<W>, omml_xml: &str) -> io::Result<()> {
    // Write the pre-serialized OMML XML directly into the stream
    writer.get_mut().write_all(omml_xml.as_bytes())?;
    Ok(())
}

fn write_footnote_ref<W: Write>(writer: &mut Writer<W>, id: u32) -> io::Result<()> {
    let id_str = id.to_string();
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:rPr").write_inner_content(|rpr| {
            rpr.create_element("w:rStyle")
                .with_attribute(("w:val", "FootnoteReference"))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("w:footnoteReference")
            .with_attribute(("w:id", id_str.as_str()))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
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
                            // Footnote reference mark at the start
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
                            // Space after reference mark
                            p_w.create_element("w:r").write_inner_content(|r_w| {
                                r_w.create_element("w:t")
                                    .with_attribute(("xml:space", "preserve"))
                                    .write_text_content(BytesText::new(" "))?;
                                Ok(())
                            })?;
                            // Content runs
                            for run in &footnote.content {
                                write_run(p_w, run)?;
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
