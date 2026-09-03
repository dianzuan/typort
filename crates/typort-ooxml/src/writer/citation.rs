use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as FmtWrite;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};

use quick_xml::Writer;
use quick_xml::events::BytesText;

use super::WriteCtx;
use super::fields::{write_fld_char, write_instruction_text_run};
use super::paragraph::write_paragraph;
use crate::document::{CitationSource, Paragraph, PersonName};

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
pub(super) fn write_citation_sdt<W: Write>(
    writer: &mut Writer<W>,
    keys: &[String],
    display_text: &str,
    sdt_id: u32,
    locale_id: u32,
) -> io::Result<()> {
    writer.create_element("w:sdt").write_inner_content(|sdt| {
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
                // instrText: CITATION key1 \l <locale> [\m key2 \m key3 ...]
                let mut instr = String::new();
                instr.push_str(" CITATION ");
                if let Some(first) = keys.first() {
                    instr.push_str(first);
                }
                let _ = write!(instr, r" \l {locale_id}");
                for key in keys.iter().skip(1) {
                    instr.push_str(r" \m ");
                    instr.push_str(key);
                }
                instr.push(' ');
                write_instruction_text_run(content, &instr)?;
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
pub(super) fn write_bibliography_sdt<W: Write>(
    writer: &mut Writer<W>,
    paragraphs: &[Paragraph],
    ctx: &WriteCtx,
) -> io::Result<()> {
    writer.create_element("w:sdt").write_inner_content(|sdt| {
        // SDT properties with bibliography marker
        sdt.create_element("w:sdtPr").write_inner_content(|pr| {
            let sdt_id = ctx.citation_id_counter.get();
            ctx.citation_id_counter.set(sdt_id + 1);
            let id_str = sdt_id.to_string();
            pr.create_element("w:id")
                .with_attribute(("w:val", id_str.as_str()))
                .write_empty()?;
            pr.create_element("w:docPartObj")
                .write_inner_content(|dpo| {
                    dpo.create_element("w:docPartGallery")
                        .with_attribute(("w:val", "Bibliographies"))
                        .write_empty()?;
                    dpo.create_element("w:docPartUnique").write_empty()?;
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
                    write_instruction_text_run(pw, " BIBLIOGRAPHY ")?;
                    // fldChar separate
                    write_fld_char(pw, "separate")?;
                    Ok(())
                })?;
                // Cached bibliography paragraphs
                for para in paragraphs {
                    write_paragraph(content, para, ctx)?;
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

/// Map a BCP 47 language tag to a Windows LCID for CITATION field codes.
pub(super) fn lang_to_lcid(lang: &str) -> u32 {
    if lang.starts_with("zh") {
        2052 // zh-CN
    } else if lang.starts_with("ja") {
        1041 // ja-JP
    } else if lang.starts_with("ko") {
        1042 // ko-KR
    } else {
        1033 // en-US (default)
    }
}

fn tag_to_guid(tag: &str) -> String {
    let mut hasher = DefaultHasher::new();
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
pub(super) fn generate_custom_xml_sources(
    writer: &mut Writer<&mut Vec<u8>>,
    sources: &[CitationSource],
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
                        .write_text_content(BytesText::new(src.source_type.ooxml_value()))?;
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
    authors: &[PersonName],
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
                                    write_optional_bib_field(
                                        p,
                                        "b:First",
                                        person.first.as_deref(),
                                    )?;
                                    write_optional_bib_field(
                                        p,
                                        "b:Middle",
                                        person.middle.as_deref(),
                                    )?;
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
pub(super) fn generate_custom_xml_item_props(
    writer: &mut Writer<&mut Vec<u8>>,
    sources: &[CitationSource],
) -> io::Result<()> {
    let guid = tag_to_guid(
        &sources
            .iter()
            .map(|s| s.tag.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    writer
        .create_element("ds:datastoreItem")
        .with_attribute(("ds:itemID", guid.as_str()))
        .with_attribute((
            "xmlns:ds",
            "http://schemas.openxmlformats.org/officeDocument/2006/customXml",
        ))
        .write_inner_content(|w| {
            w.create_element("ds:schemaRefs")
                .write_inner_content(|sr| {
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
pub(super) fn generate_custom_xml_rels(writer: &mut Writer<&mut Vec<u8>>) -> io::Result<()> {
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
