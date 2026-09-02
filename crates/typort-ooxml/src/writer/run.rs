use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::BytesText;

use super::math::write_math_inline;
use super::package::{write_font_triple, write_size_pair};
use crate::document::{Document, DocumentStyle, InlineElement, Run};

pub(super) fn write_inline<W: Write>(
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
pub(super) fn write_tab<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:tab").write_empty()?;
        Ok(())
    })?;
    Ok(())
}

pub(super) fn write_page_break<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:br")
            .with_attribute(("w:type", "page"))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
}

pub(super) fn write_column_break<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:br")
            .with_attribute(("w:type", "column"))
            .write_empty()?;
        Ok(())
    })?;
    Ok(())
}

pub(super) fn write_text_run<W: Write>(
    writer: &mut Writer<W>,
    text: &str,
    preserve_space: bool,
) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        let mut text_element = w.create_element("w:t");
        if preserve_space {
            text_element = text_element.with_attribute(("xml:space", "preserve"));
        }
        text_element.write_text_content(BytesText::new(text))?;
        Ok(())
    })?;
    Ok(())
}

/// Write a forced line break run: `<w:r><w:br/></w:r>`.
fn write_line_break<W: Write>(writer: &mut Writer<W>) -> io::Result<()> {
    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:br").write_empty()?;
        Ok(())
    })?;
    Ok(())
}

pub(super) fn write_run<W: Write>(
    writer: &mut Writer<W>,
    run: &Run,
    doc_style: &DocumentStyle,
) -> io::Result<()> {
    if run.line_break {
        return write_line_break(writer);
    }
    writer.create_element("w:r").write_inner_content(|w| {
        let has_font_override = run.font_ascii.is_some() || run.font_east_asia.is_some();
        let has_rpr = !run.format_key().is_plain();
        if has_rpr {
            w.create_element("w:rPr").write_inner_content(|rpr| {
                if run.monospace {
                    write_font_triple(
                        rpr,
                        doc_style.code_font.as_str(),
                        doc_style.code_font.as_str(),
                        false,
                    )?;
                } else if has_font_override {
                    let ascii = run
                        .font_ascii
                        .as_deref()
                        .unwrap_or(&doc_style.body_font_ascii);
                    let east_asia = run
                        .font_east_asia
                        .as_deref()
                        .unwrap_or(&doc_style.body_font_east_asia);
                    write_font_triple(rpr, ascii, east_asia, false)?;
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
                // A super/subscript run must not carry an explicit reduced size:
                // `w:vertAlign` already shrinks the glyph and raises it by a fraction
                // of the *effective* em, so a pre-shrunk `w:sz` collapses the raise and
                // the mark sits mid-line. Emit vertAlign alone and let the consumer
                // reduce+raise from the inherited body size (ECMA-376 §17.3.2.42).
                if let Some(size) = run
                    .size_half_pt
                    .filter(|_| !run.superscript && !run.subscript)
                {
                    let size_str = size.to_string();
                    write_size_pair(rpr, size_str.as_str())?;
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
