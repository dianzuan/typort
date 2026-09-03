use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::BytesText;

use super::package::write_value_element;
use super::run::{write_inline, write_text_run};
use crate::document::{Document, FootnoteFormat};

/// Write `w:footnotePr` element with optional circled-number format and per-page restart.
pub(super) fn write_footnote_pr<W: Write>(
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
fn write_footnote_mark_run<W: Write>(
    writer: &mut Writer<W>,
    mark: Option<&str>,
    preserve_space: bool,
) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:rPr").write_inner_content(|rpr| {
            write_value_element(rpr, "w:rStyle", "FootnoteReference")?;
            Ok(())
        })?;
        if let Some(mark) = mark {
            let mut text = w.create_element("w:t");
            if preserve_space {
                text = text.with_attribute(("xml:space", "preserve"));
            }
            text.write_text_content(BytesText::new(mark))?;
        } else {
            w.create_element("w:footnoteRef").write_empty()?;
        }
        Ok(())
    })?;
    Ok(())
}

pub(super) fn write_footnote_ref<W: Write>(
    writer: &mut Writer<W>,
    id: u32,
    footnote_format: &FootnoteFormat,
) -> io::Result<()> {
    let id_str = id.to_string();
    let use_custom = *footnote_format == FootnoteFormat::CircledNumber;

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
        write_footnote_mark_run(writer, Some(&mark), true)?;
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

pub(super) fn generate_footnotes_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    doc: &Document,
) -> io::Result<()> {
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
                            let use_custom =
                                doc.style.footnote_format == FootnoteFormat::CircledNumber;
                            if use_custom {
                                let seq = footnote.id - 1;
                                let mark = circled_number_char(seq);
                                write_footnote_mark_run(p_w, Some(&mark), false)?;
                            } else {
                                write_footnote_mark_run(p_w, None, false)?;
                            }
                            // Space after reference mark
                            write_text_run(p_w, " ", true)?;
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
