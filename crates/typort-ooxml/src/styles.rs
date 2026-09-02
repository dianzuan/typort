use quick_xml::Writer;
use std::io::{self, Write};

use crate::document::DocumentStyle;
use crate::writer::{
    two_em_hanging_twips, write_font_triple, write_indentation, write_language_pair,
    write_size_pair,
};

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
            write_style_code_block(w, style)?;
            write_style_bibliography(w, style)?;
            if has_footnotes {
                write_style_footnote_reference(w)?;
                write_style_footnote_text(w, style)?;
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
                    write_font_triple(
                        rpr,
                        style.body_font_ascii.as_str(),
                        style.body_font_east_asia.as_str(),
                        style.has_cjk_content,
                    )?;
                    rpr.create_element("w:kern")
                        .with_attribute(("w:val", "2"))
                        .write_empty()?;
                    write_size_pair(rpr, sz.as_str())?;
                    write_language_pair(
                        rpr,
                        "w:lang",
                        style.lang_latin.as_str(),
                        style.lang_east_asia.as_str(),
                    )?;
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
                write_font_triple(
                    rpr,
                    style.body_font_ascii.as_str(),
                    style.body_font_east_asia.as_str(),
                    style.has_cjk_content,
                )?;
                write_size_pair(rpr, sz.as_str())?;
                write_language_pair(
                    rpr,
                    "w:lang",
                    style.lang_latin.as_str(),
                    style.lang_east_asia.as_str(),
                )?;
                Ok(())
            })?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                ppr.create_element("w:widowControl").write_empty()?;
                ppr.create_element("w:jc")
                    .with_attribute(("w:val", style.body_alignment.as_str()))
                    .write_empty()?;
                let sp_before = style.body_spacing_before.to_string();
                let sp_after = style.body_spacing_after.to_string();
                // No w:line/w:lineRule: Pandoc-aligned. Emitting the rendered
                // line pitch as lineRule="atLeast" showed up in Word as
                // 行距=最小值; instead set only paragraph before/after and let
                // Word default to single line spacing (one click to 1.5x if the
                // author wants it).
                ppr.create_element("w:spacing")
                    .with_attribute(("w:before", sp_before.as_str()))
                    .with_attribute(("w:after", sp_after.as_str()))
                    .write_empty()?;
                // East-Asian char-based first-line indent: Word prefers
                // firstLineChars over firstLine, so emit it first and keep the
                // twips as a fallback. Absolute indents emit only firstLine.
                let chars_str = style.first_line_indent_chars.map(|chars| chars.to_string());
                write_indentation(ppr, None, None, chars_str.as_deref(), Some(indent.as_str()))?;
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

    let idx = (level as usize).saturating_sub(1).min(4);
    let heading_size = style.heading_sizes[idx];
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
                // No w:keepNext: Typst headings are sticky, but pinning a
                // heading to its following content makes Word push the heading
                // onto the next page when the group doesn't fit at the bottom,
                // leaving a gap. Let Word flow headings so the space is filled.
                ppr.create_element("w:widowControl").write_empty()?;
                let outline_level = (level - 1).to_string();
                ppr.create_element("w:outlineLvl")
                    .with_attribute(("w:val", outline_level.as_str()))
                    .write_empty()?;
                let sp_before = style.heading_spacing_before[idx].to_string();
                let sp_after = style.heading_spacing_after[idx].to_string();
                ppr.create_element("w:spacing")
                    .with_attribute(("w:before", sp_before.as_str()))
                    .with_attribute(("w:after", sp_after.as_str()))
                    .write_empty()?;
                write_indentation(ppr, None, None, None, Some("0"))?;
                Ok(())
            })?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                write_font_triple(
                    rpr,
                    style.body_font_ascii.as_str(),
                    style.body_font_east_asia.as_str(),
                    false,
                )?;
                rpr.create_element("w:b").write_empty()?;
                write_size_pair(rpr, font_size.as_str())?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

/// Define the `Bibliography` paragraph style. The reference list's field-code
/// paragraph carries this (built-in) style id; defining it explicitly makes the
/// document self-contained (not reliant on a host editor's built-in definition),
/// so it renders consistently across word processors. Based on `Normal` with a
/// hanging indent matching the reference layout.
fn write_style_bibliography<W: Write>(w: &mut Writer<W>, style: &DocumentStyle) -> io::Result<()> {
    let hang = two_em_hanging_twips(style.body_size_half_pt).to_string();
    w.create_element("w:style")
        .with_attribute(("w:type", "paragraph"))
        .with_attribute(("w:styleId", "Bibliography"))
        .write_inner_content(|s| {
            s.create_element("w:name")
                .with_attribute(("w:val", "Bibliography"))
                .write_empty()?;
            s.create_element("w:basedOn")
                .with_attribute(("w:val", "Normal"))
                .write_empty()?;
            s.create_element("w:pPr").write_inner_content(|ppr| {
                write_indentation(
                    ppr,
                    Some(hang.as_str()),
                    Some(hang.as_str()),
                    None,
                    Some("0"),
                )?;
                Ok(())
            })?;
            Ok(())
        })?;
    Ok(())
}

fn write_style_code_block<W: Write>(w: &mut Writer<W>, style: &DocumentStyle) -> io::Result<()> {
    let code_sz = style.code_size_half_pt.to_string();
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
                write_indentation(ppr, None, None, None, Some("0"))?;
                ppr.create_element("w:spacing")
                    .with_attribute(("w:line", "240"))
                    .with_attribute(("w:lineRule", "auto"))
                    .with_attribute(("w:before", "0"))
                    .with_attribute(("w:after", "0"))
                    .write_empty()?;
                // Light-gray background. This is a Word convention for code
                // blocks, not a property Typst exposes (raw blocks carry no fill
                // in the AST/PagedDocument), so it is a fixed style default.
                ppr.create_element("w:shd")
                    .with_attribute(("w:val", "clear"))
                    .with_attribute(("w:color", "auto"))
                    .with_attribute(("w:fill", "F2F2F2"))
                    .write_empty()?;
                Ok(())
            })?;
            s.create_element("w:rPr").write_inner_content(|rpr| {
                write_font_triple(
                    rpr,
                    style.code_font.as_str(),
                    style.code_font.as_str(),
                    false,
                )?;
                write_size_pair(rpr, code_sz.as_str())?;
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

fn write_style_footnote_text<W: Write>(w: &mut Writer<W>, style: &DocumentStyle) -> io::Result<()> {
    let fn_sz = style.footnote_size_half_pt.to_string();
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
                write_size_pair(rpr, fn_sz.as_str())?;
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

    // Determine East Asian charset from language tag:
    // zh → "86" (GB2312), ja → "80" (Shift-JIS), ko → "81" (Hangul), else → "00" (ANSI)
    let ea_charset = if style.lang_east_asia.starts_with("zh") {
        "86"
    } else if style.lang_east_asia.starts_with("ja") {
        "80"
    } else if style.lang_east_asia.starts_with("ko") {
        "81"
    } else {
        "00"
    };

    // Always include the body fonts
    fonts.push((style.body_font_ascii.as_str(), "00"));
    if style.body_font_east_asia != style.body_font_ascii {
        fonts.push((style.body_font_east_asia.as_str(), ea_charset));
    }

    // Include the detected code font
    fonts.push((style.code_font.as_str(), "00"));

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
