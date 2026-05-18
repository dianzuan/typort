use quick_xml::Writer;
use std::io::{self, Write};

use crate::document::DocumentStyle;

pub(crate) fn generate_styles(
    writer: &mut Writer<&mut Vec<u8>>,
    has_footnotes: bool,
    style: &DocumentStyle,
) -> io::Result<()> {
    writer
        .create_element("w:styles")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            write_doc_defaults(w, style)?;
            write_style_normal(w, style)?;
            for level in 1..=5 {
                write_style_heading(w, level, style)?;
            }
            write_style_code_block(w)?;
            if has_footnotes {
                write_style_footnote_reference(w)?;
                write_style_footnote_text(w)?;
            }
            Ok(())
        })?;
    Ok(())
}

fn write_doc_defaults<W: Write>(w: &mut Writer<W>, style: &DocumentStyle) -> io::Result<()> {
    let sz = style.body_size_half_pt.to_string();
    w.create_element("w:docDefaults").write_inner_content(|d| {
        d.create_element("w:rPrDefault")
            .write_inner_content(|rprd| {
                rprd.create_element("w:rPr").write_inner_content(|rpr| {
                    rpr.create_element("w:rFonts")
                        .with_attribute(("w:ascii", style.body_font_ascii.as_str()))
                        .with_attribute(("w:hAnsi", style.body_font_ascii.as_str()))
                        .with_attribute(("w:eastAsia", style.body_font_east_asia.as_str()))
                        .with_attribute(("w:hint", "eastAsia"))
                        .write_empty()?;
                    rpr.create_element("w:kern")
                        .with_attribute(("w:val", "2"))
                        .write_empty()?;
                    rpr.create_element("w:sz")
                        .with_attribute(("w:val", sz.as_str()))
                        .write_empty()?;
                    rpr.create_element("w:szCs")
                        .with_attribute(("w:val", sz.as_str()))
                        .write_empty()?;
                    rpr.create_element("w:lang")
                        .with_attribute(("w:val", "en-US"))
                        .with_attribute(("w:eastAsia", "zh-CN"))
                        .write_empty()?;
                    Ok(())
                })?;
                Ok(())
            })?;
        d.create_element("w:pPrDefault")
            .write_inner_content(|pprd| {
                pprd.create_element("w:pPr").write_inner_content(|ppr| {
                    // CJK typography attributes — inherited by all paragraphs
                    ppr.create_element("w:kinsoku").write_empty()?;
                    ppr.create_element("w:overflowPunct").write_empty()?;
                    ppr.create_element("w:autoSpaceDE").write_empty()?;
                    ppr.create_element("w:autoSpaceDN").write_empty()?;
                    ppr.create_element("w:wordWrap").write_empty()?;
                    ppr.create_element("w:topLinePunct").write_empty()?;
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    })?;
    Ok(())
}

fn write_style_normal<W: Write>(w: &mut Writer<W>, style: &DocumentStyle) -> io::Result<()> {
    let sz = style.body_size_half_pt.to_string();
    let line_spacing = style.line_spacing.to_string();
    let indent = style.first_line_indent_twips.to_string();
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
                    .with_attribute(("w:ascii", style.body_font_ascii.as_str()))
                    .with_attribute(("w:hAnsi", style.body_font_ascii.as_str()))
                    .with_attribute(("w:eastAsia", style.body_font_east_asia.as_str()))
                    .with_attribute(("w:hint", "eastAsia"))
                    .write_empty()?;
                rpr.create_element("w:sz")
                    .with_attribute(("w:val", sz.as_str()))
                    .write_empty()?;
                rpr.create_element("w:szCs")
                    .with_attribute(("w:val", sz.as_str()))
                    .write_empty()?;
                rpr.create_element("w:lang")
                    .with_attribute(("w:val", "en-US"))
                    .with_attribute(("w:eastAsia", "zh-CN"))
                    .write_empty()?;
                Ok(())
            })?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                ppr.create_element("w:widowControl").write_empty()?;
                ppr.create_element("w:jc")
                    .with_attribute(("w:val", "both"))
                    .write_empty()?;
                ppr.create_element("w:spacing")
                    .with_attribute(("w:line", line_spacing.as_str()))
                    .with_attribute(("w:lineRule", "auto"))
                    .write_empty()?;
                ppr.create_element("w:ind")
                    .with_attribute(("w:firstLine", indent.as_str()))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_heading<W: Write>(
    w: &mut Writer<W>,
    level: u8,
    style: &DocumentStyle,
) -> io::Result<()> {
    let style_id = format!("Heading{level}");
    let name = format!("heading {level}");

    // Heading sizes: scale up from body size
    // Level 1: body + 4.5pt (9 half-pt), Level 2: body + 3.5pt (7),
    // Level 3: body + 2.5pt (5), Level 4: body + 1.5pt (3), Level 5: body + 0.5pt (1)
    let heading_size = style.body_size_half_pt
        + match level {
            1 => 9,
            2 => 7,
            3 => 5,
            4 => 3,
            _ => 1,
        };
    let font_size = heading_size.to_string();

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
                ppr.create_element("w:keepNext").write_empty()?;
                ppr.create_element("w:widowControl").write_empty()?;
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
                    .with_attribute(("w:ascii", style.body_font_ascii.as_str()))
                    .with_attribute(("w:hAnsi", style.body_font_ascii.as_str()))
                    .with_attribute(("w:eastAsia", style.body_font_east_asia.as_str()))
                    .write_empty()?;
                rpr.create_element("w:b").write_empty()?;
                rpr.create_element("w:sz")
                    .with_attribute(("w:val", font_size.as_str()))
                    .write_empty()?;
                rpr.create_element("w:szCs")
                    .with_attribute(("w:val", font_size.as_str()))
                    .write_empty()?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_code_block<W: Write>(w: &mut Writer<W>) -> io::Result<()> {
    w.create_element("w:style")
        .with_attribute(("w:type", "paragraph"))
        .with_attribute(("w:styleId", "CodeBlock"))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", "Code Block"))
                .write_empty()?;
            s.create_element("w:basedOn")
                .with_attribute(("w:val", "Normal"))
                .write_empty()?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                ppr.create_element("w:ind")
                    .with_attribute(("w:firstLine", "0"))
                    .write_empty()?;
                ppr.create_element("w:spacing")
                    .with_attribute(("w:line", "240"))
                    .with_attribute(("w:lineRule", "auto"))
                    .with_attribute(("w:before", "0"))
                    .with_attribute(("w:after", "0"))
                    .write_empty()?;
                ppr.create_element("w:shd")
                    .with_attribute(("w:val", "clear"))
                    .with_attribute(("w:color", "auto"))
                    .with_attribute(("w:fill", "F2F2F2"))
                    .write_empty()?;
                Ok(())
            })?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                rpr.create_element("w:rFonts")
                    .with_attribute(("w:ascii", "Courier New"))
                    .with_attribute(("w:hAnsi", "Courier New"))
                    .with_attribute(("w:eastAsia", "\u{7b49}\u{7ebf}"))
                    .write_empty()?;
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

pub(crate) fn generate_font_table(
    writer: &mut Writer<&mut Vec<u8>>,
    style: &DocumentStyle,
) -> io::Result<()> {
    // Collect unique fonts to declare in fontTable.xml
    let mut fonts: Vec<(&str, &str)> = Vec::new();

    // Always include the body fonts
    fonts.push((style.body_font_ascii.as_str(), "00"));
    if style.body_font_east_asia != style.body_font_ascii {
        fonts.push((style.body_font_east_asia.as_str(), "86"));
    }

    // Always include code block fonts
    fonts.push(("Courier New", "00"));
    fonts.push(("\u{7b49}\u{7ebf}", "86")); // 等线

    // Deduplicate by font name
    fonts.sort_by_key(|(name, _)| *name);
    fonts.dedup_by_key(|(name, _)| *name);

    writer
        .create_element("w:fonts")
        .with_attribute((
            "xmlns:w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ))
        .write_inner_content(|w| {
            for (name, charset) in &fonts {
                w.create_element("w:font")
                    .with_attribute(("w:name", *name))
                    .write_inner_content(|f| {
                        f.create_element("w:charset")
                            .with_attribute(("w:val", *charset))
                            .write_empty()?;
                        Ok(())
                    })?;
            }
            Ok(())
        })?;
    Ok(())
}
