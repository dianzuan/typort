use std::io::{self, Write};

use quick_xml::Writer;

use super::package::write_value_element;
use crate::document::Document;

#[allow(
    clippy::too_many_lines,
    reason = "writes all numbering levels in schema order"
)]
pub(super) fn generate_numbering_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    doc: &Document,
) -> io::Result<()> {
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
                    write_numbering_level(abs, "0", "1", "decimal", "%1.", "left")?;
                    Ok(())
                })?;
            // Abstract numbering 2: unordered list (bullet)
            w.create_element("w:abstractNum")
                .with_attribute(("w:abstractNumId", "2"))
                .write_inner_content(|abs| {
                    write_numbering_level(abs, "0", "1", "bullet", "\u{2022}", "left")?;
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
            write_numbering_instance(w, "1", "1", None)?;
            // Numbering instance 2 -> abstractNum 2 (unordered)
            write_numbering_instance(w, "2", "2", None)?;
            // Numbering instance 3 -> abstractNum 3 (Chinese headings, opt-in)
            write_numbering_instance(w, "3", "3", None)?;
            // Dynamic numbering instances for each top-level list
            for &(num_id, abstract_num_id, start) in &doc.list_num_instances {
                let num_id_str = num_id.to_string();
                let abs_id_str = abstract_num_id.to_string();
                let start_str = start.to_string();
                write_numbering_instance(
                    w,
                    num_id_str.as_str(),
                    abs_id_str.as_str(),
                    Some(start_str.as_str()),
                )?;
            }
            Ok(())
        })?;
    Ok(())
}

fn write_numbering_instance<W: Write>(
    writer: &mut Writer<W>,
    num_id: &str,
    abstract_num_id: &str,
    start: Option<&str>,
) -> io::Result<()> {
    writer
        .create_element("w:num")
        .with_attribute(("w:numId", num_id))
        .write_inner_content(|num| {
            write_value_element(num, "w:abstractNumId", abstract_num_id)?;
            if let Some(start) = start {
                // Each top-level list is an independent instance: override the
                // level-0 start so Word restarts at the list's own first number.
                num.create_element("w:lvlOverride")
                    .with_attribute(("w:ilvl", "0"))
                    .write_inner_content(|ovr| {
                        write_value_element(ovr, "w:startOverride", start)?;
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
            write_value_element(lvl, "w:start", start)?;
            write_value_element(lvl, "w:numFmt", num_fmt)?;
            write_value_element(lvl, "w:lvlText", lvl_text)?;
            write_value_element(lvl, "w:lvlJc", jc)?;
            Ok(())
        })?;
    Ok(())
}
