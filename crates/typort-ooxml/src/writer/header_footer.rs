use std::io::{self, Write};

use quick_xml::Writer;

use super::fields::write_complex_field;
use super::run::write_run;
use crate::document::{DocumentStyle, HeaderFooter, InlineElement, Paragraph};

pub(super) fn generate_header_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    content: &HeaderFooter,
    doc_style: &DocumentStyle,
) -> io::Result<()> {
    generate_header_footer_part(writer, "w:hdr", content, doc_style)
}

pub(super) fn generate_footer_xml(
    writer: &mut Writer<&mut Vec<u8>>,
    content: &HeaderFooter,
    doc_style: &DocumentStyle,
) -> io::Result<()> {
    generate_header_footer_part(writer, "w:ftr", content, doc_style)
}

/// Generate a footer XML part containing a PAGE field code for automatic page numbering.
///
/// Produces a centered paragraph with `fldChar begin / instrText PAGE / fldChar separate /
/// fallback text / fldChar end`.
pub(super) fn generate_page_number_footer_xml(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
                write_complex_field(pw, " PAGE ", "1", false)?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn generate_header_footer_part(
    writer: &mut Writer<&mut Vec<u8>>,
    root_tag: &str,
    content: &HeaderFooter,
    doc_style: &DocumentStyle,
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
    para: &Paragraph,
    doc_style: &DocumentStyle,
) -> io::Result<()> {
    writer.create_element("w:p").write_inner_content(|w| {
        let has_alignment = para.alignment.is_some();
        if has_alignment {
            w.create_element("w:pPr").write_inner_content(|ppr| {
                if let Some(alignment) = &para.alignment {
                    ppr.create_element("w:jc")
                        .with_attribute(("w:val", alignment.ooxml_value()))
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
