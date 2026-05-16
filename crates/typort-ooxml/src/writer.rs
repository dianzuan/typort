use std::io::{self, Seek, Write};

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{Alignment, BlockElement, Document, InlineElement, ParagraphStyle, Table};
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

    zip.start_file("word/settings.xml", options)?;
    zip.write_all(&xml_part(generate_settings)?)?;

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
                .with_attribute(("PartName", "/word/settings.xml"))
                .with_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"))
                .write_empty()?;
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
            let settings_id = match (has_footnotes, has_numbering) {
                (true, true) => "rId5",
                (true, false) | (false, true) => "rId4",
                (false, false) => "rId3",
            };
            w.create_element("Relationship")
                .with_attribute(("Id", settings_id))
                .with_attribute((
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings",
                ))
                .with_attribute(("Target", "settings.xml"))
                .write_empty()?;
            Ok(())
        })?;
    Ok(())
}

fn generate_settings(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
    writer
        .create_element("w:settings")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            // Footnote properties: circled numbers (chicago) with per-page restart
            w.create_element("w:footnotePr").write_inner_content(|fp| {
                fp.create_element("w:numFmt")
                    .with_attribute(("w:val", "chicago"))
                    .write_empty()?;
                fp.create_element("w:numRestart")
                    .with_attribute(("w:val", "eachPage"))
                    .write_empty()?;
                Ok(())
            })?;
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
                .with_attribute(("w:val", "en-US"))
                .with_attribute(("w:eastAsia", "zh-CN"))
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
        // Footnote properties in section: circled numbers with per-page restart
        w.create_element("w:footnotePr").write_inner_content(|fp| {
            fp.create_element("w:numFmt")
                .with_attribute(("w:val", "chicago"))
                .write_empty()?;
            fp.create_element("w:numRestart")
                .with_attribute(("w:val", "eachPage"))
                .write_empty()?;
            Ok(())
        })?;
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

#[allow(clippy::too_many_lines)]
fn write_paragraph<W: Write>(
    writer: &mut Writer<W>,
    para: &crate::document::Paragraph,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        let has_style = para.style.is_some();
        let has_list = para.list_id.is_some();
        let has_alignment = para.alignment.is_some();
        let has_left_indent = para.left_indent.is_some();
        let has_code_block = para.code_block;
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
        if has_style
            || has_list
            || has_alignment
            || suppress_indent
            || has_eq_number
            || has_hanging
            || has_left_indent
            || has_code_block
        {
            w.create_element("w:pPr").write_inner_content(|ppr| {
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
                // Emit tab stops for numbered equations (right-aligned at page width)
                if has_eq_number {
                    ppr.create_element("w:tabs").write_inner_content(|tabs| {
                        tabs.create_element("w:tab")
                            .with_attribute(("w:val", "right"))
                            .with_attribute(("w:pos", "8306"))
                            .write_empty()?;
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
                    ppr.create_element("w:ind")
                        .with_attribute(("w:left", "420"))
                        .with_attribute(("w:hanging", "420"))
                        .with_attribute(("w:firstLine", "0"))
                        .write_empty()?;
                } else if has_list {
                    ppr.create_element("w:ind")
                        .with_attribute(("w:left", "720"))
                        .with_attribute(("w:hanging", "360"))
                        .write_empty()?;
                } else if suppress_indent || has_eq_number {
                    ppr.create_element("w:ind")
                        .with_attribute(("w:firstLine", "0"))
                        .write_empty()?;
                }
                // Emit alignment
                if let Some(alignment) = &para.alignment {
                    let val = match alignment {
                        Alignment::Left => "left",
                        Alignment::Center => "center",
                        Alignment::Right => "right",
                        Alignment::Justify => "both",
                    };
                    ppr.create_element("w:jc")
                        .with_attribute(("w:val", val))
                        .write_empty()?;
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
                    InlineElement::Math {
                        omml_xml,
                        equation_number,
                    } => {
                        write_math_inline(w, omml_xml)?;
                        if let Some(num) = equation_number {
                            write_equation_number(w, num)?;
                        }
                    }
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn write_run<W: Write>(writer: &mut Writer<W>, run: &crate::document::Run) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        let has_rpr = run.bold || run.italic || run.superscript || run.subscript || run.monospace;
        if has_rpr {
            w.create_element("w:rPr").write_inner_content(|rpr| {
                if run.monospace {
                    rpr.create_element("w:rFonts")
                        .with_attribute(("w:ascii", "Courier New"))
                        .with_attribute(("w:hAnsi", "Courier New"))
                        .with_attribute(("w:eastAsia", "\u{7b49}\u{7ebf}"))
                        .write_empty()?;
                }
                if run.bold {
                    rpr.create_element("w:b").write_empty()?;
                }
                if run.italic {
                    rpr.create_element("w:i").write_empty()?;
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

fn write_table<W: Write>(writer: &mut Writer<W>, table: &Table) -> io::Result<()> {
    // Determine number of columns from the first row for equal-width distribution
    let num_cols = table
        .rows
        .first()
        .map_or(1, |r| r.cells.iter().map(|c| c.colspan).sum::<u32>());

    writer.create_element("w:tbl").write_inner_content(|w| {
        // Table properties with borders
        w.create_element("w:tblPr").write_inner_content(|tpr| {
            tpr.create_element("w:tblW")
                .with_attribute(("w:w", "5000"))
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

#[allow(clippy::too_many_lines)]
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
