use quick_xml::Writer;
use std::io::{self, Write};

pub(crate) fn generate_styles(
    writer: &mut Writer<&mut Vec<u8>>,
    has_footnotes: bool,
) -> io::Result<()> {
    writer
        .create_element("w:styles")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            write_doc_defaults(w)?;
            write_style_normal(w)?;
            for level in 1..=5 {
                write_style_heading(w, level)?;
            }
            if has_footnotes {
                write_style_footnote_reference(w)?;
                write_style_footnote_text(w)?;
            }
            Ok(())
        })?;
    Ok(())
}

fn write_doc_defaults<W: Write>(w: &mut Writer<W>) -> io::Result<()> {
    w.create_element("w:docDefaults").write_inner_content(|d| {
        d.create_element("w:rPrDefault")
            .write_inner_content(|rprd| {
                rprd.create_element("w:rPr").write_inner_content(|rpr| {
                    rpr.create_element("w:rFonts")
                        .with_attribute(("w:ascii", "Times New Roman"))
                        .with_attribute(("w:hAnsi", "Times New Roman"))
                        .with_attribute(("w:eastAsia", "\u{5b8b}\u{4f53}"))
                        .write_empty()?;
                    rpr.create_element("w:kern")
                        .with_attribute(("w:val", "2"))
                        .write_empty()?;
                    rpr.create_element("w:sz")
                        .with_attribute(("w:val", "21"))
                        .write_empty()?;
                    rpr.create_element("w:szCs")
                        .with_attribute(("w:val", "21"))
                        .write_empty()?;
                    rpr.create_element("w:lang")
                        .with_attribute(("w:val", "en-US"))
                        .with_attribute(("w:eastAsia", "zh-CN"))
                        .write_empty()?;
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    })?;
    Ok(())
}

fn write_style_normal<W: Write>(w: &mut Writer<W>) -> io::Result<()> {
    w.create_element("w:style")
        .with_attribute(("w:type", "paragraph"))
        .with_attribute(("w:default", "1"))
        .with_attribute(("w:styleId", "Normal"))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", "Normal"))
                .write_empty()?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:rFonts")
                    .with_attribute(("w:ascii", "Times New Roman"))
                    .with_attribute(("w:hAnsi", "Times New Roman"))
                    .with_attribute(("w:eastAsia", "\u{5b8b}\u{4f53}"))
                    .write_empty()?;
                rpr.create_element("w:sz")
                    .with_attribute(("w:val", "21"))
                    .write_empty()?;
                rpr.create_element("w:szCs")
                    .with_attribute(("w:val", "21"))
                    .write_empty()?;
                rpr.create_element("w:lang")
                    .with_attribute(("w:val", "en-US"))
                    .with_attribute(("w:eastAsia", "zh-CN"))
                    .write_empty()?;
                Ok(())
            })?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                ppr.create_element("w:spacing")
                    .with_attribute(("w:line", "360"))
                    .with_attribute(("w:lineRule", "auto"))
                    .write_empty()?;
                ppr.create_element("w:ind")
                    .with_attribute(("w:firstLine", "420"))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_heading<W: Write>(w: &mut Writer<W>, level: u8) -> io::Result<()> {
    let style_id = format!("Heading{level}");
    let name = format!("heading {level}");
    let font_size = match level {
        1 => "30", // 15pt
        2 => "28", // 14pt
        3 => "26", // 13pt
        4 => "24", // 12pt
        _ => "22", // 11pt
    };

    w.create_element("w:style")
        .with_attribute(("w:type", "paragraph"))
        .with_attribute(("w:styleId", style_id.as_str()))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", name.as_str()))
                .write_empty()?;
            s.create_element("w:basedOn")
                .with_attribute(("w:val", "Normal"))
                .write_empty()?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                let outline_level = (level - 1).to_string();
                ppr.create_element("w:outlineLvl")
                    .with_attribute(("w:val", outline_level.as_str()))
                    .write_empty()?;
                ppr.create_element("w:spacing")
                    .with_attribute(("w:before", "240"))
                    .with_attribute(("w:after", "120"))
                    .write_empty()?;
                ppr.create_element("w:ind")
                    .with_attribute(("w:firstLine", "0"))
                    .write_empty()?;
                Ok(())
            })?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:rFonts")
                    .with_attribute(("w:ascii", "Times New Roman"))
                    .with_attribute(("w:hAnsi", "Times New Roman"))
                    .with_attribute(("w:eastAsia", "\u{9ed1}\u{4f53}"))
                    .write_empty()?;
                rpr.create_element("w:b").write_empty()?;
                rpr.create_element("w:sz")
                    .with_attribute(("w:val", font_size))
                    .write_empty()?;
                rpr.create_element("w:szCs")
                    .with_attribute(("w:val", font_size))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_footnote_reference<W: Write>(w: &mut Writer<W>) -> io::Result<()> {
    w.create_element("w:style")
        .with_attribute(("w:type", "character"))
        .with_attribute(("w:styleId", "FootnoteReference"))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", "footnote reference"))
                .write_empty()?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:vertAlign")
                    .with_attribute(("w:val", "superscript"))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_footnote_text<W: Write>(w: &mut Writer<W>) -> io::Result<()> {
    w.create_element("w:style")
        .with_attribute(("w:type", "paragraph"))
        .with_attribute(("w:styleId", "FootnoteText"))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", "footnote text"))
                .write_empty()?;
            s.create_element("w:basedOn")
                .with_attribute(("w:val", "Normal"))
                .write_empty()?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:sz")
                    .with_attribute(("w:val", "18"))
                    .write_empty()?;
                rpr.create_element("w:szCs")
                    .with_attribute(("w:val", "18"))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

pub(crate) fn generate_font_table(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
    writer
        .create_element("w:fonts")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            for (name, charset) in [
                ("Times New Roman", "00"),
                ("\u{5b8b}\u{4f53}", "86"), // 宋体
                ("\u{9ed1}\u{4f53}", "86"), // 黑体
                ("\u{6977}\u{4f53}", "86"), // 楷体
                ("\u{4eff}\u{5b8b}", "86"), // 仿宋
            ] {
                w.create_element("w:font")
                    .with_attribute(("w:name", name))
                    .write_inner_content(|f| {
                        f.create_element("w:charset")
                            .with_attribute(("w:val", charset))
                            .write_empty()?;
                        Ok(())
                    })?;
            }
            Ok(())
        })?;
    Ok(())
}
