use std::cell::Cell;
use std::error::Error;
use std::io::{Seek, Write};

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::document::{BlockElement, Document, DocumentStyle};
use crate::styles;

mod citation;
mod document;
mod fields;
mod footnotes;
mod header_footer;
mod image;
mod math;
mod numbering;
pub(crate) mod package;
mod paragraph;
mod run;
mod table;

use citation::{
    generate_custom_xml_item_props, generate_custom_xml_rels, generate_custom_xml_sources,
};
use document::generate_document_xml;
use footnotes::generate_footnotes_xml;
use header_footer::{generate_footer_xml, generate_header_xml, generate_page_number_footer_xml};
use image::collect_images;
use numbering::generate_numbering_xml;
use package::{
    generate_content_types, generate_core_properties, generate_document_rels, generate_rels,
    generate_settings, xml_part,
};

/// Tracks which optional document parts are present, used for rId assignment
/// and conditional XML generation.
struct DocParts {
    relationships: Vec<RelKind>,
}

impl DocParts {
    fn new(doc: &Document) -> Self {
        let mut relationships = vec![RelKind::Styles, RelKind::FontTable];
        if !doc.footnotes.is_empty() {
            relationships.push(RelKind::Footnotes);
        }
        if doc_has_lists(doc) {
            relationships.push(RelKind::Numbering);
        }
        relationships.push(RelKind::Settings);
        if doc.header.is_some() {
            relationships.push(RelKind::Header);
        }
        if doc.footer.is_some() || doc.page_numbering.is_some() {
            relationships.push(RelKind::Footer);
        }
        if !doc.citation_sources.is_empty() {
            relationships.push(RelKind::Bibliography);
        }
        Self { relationships }
    }

    fn has(&self, kind: RelKind) -> bool {
        self.relationships.contains(&kind)
    }
}

/// Read-only context threaded through the body writers (`write_paragraph`,
/// `write_table`, `write_bibliography_sdt`), replacing the
/// `(doc_style, parts, content_width, image_counter, citation_id_counter)` tuple
/// that previously appeared on each. All fields are shared, so this is passed
/// by `&WriteCtx`; the `Cell`s carry the running image/citation ids.
struct WriteCtx<'a> {
    doc_style: &'a DocumentStyle,
    parts: &'a DocParts,
    content_width_twips: u32,
    image_counter: &'a Cell<usize>,
    citation_id_counter: &'a Cell<u32>,
}

/// Write a Document to a .docx file (ZIP archive) into the given writer.
///
/// # Errors
/// Returns an error if XML serialization or ZIP writing fails.
pub fn write_docx<W: Write + Seek>(doc: &Document, writer: W) -> Result<(), Box<dyn Error>> {
    let parts = DocParts::new(doc);
    let images = collect_images(doc);
    let has_images = !images.is_empty();

    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(&xml_part(|w| generate_content_types(w, &parts, &images))?)?;

    zip.start_file("_rels/.rels", options)?;
    zip.write_all(&xml_part(generate_rels)?)?;

    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(&xml_part(|w| generate_document_rels(w, &parts, &images))?)?;

    zip.start_file("word/styles.xml", options)?;
    zip.write_all(&xml_part(|w| {
        styles::generate_styles(w, parts.has(RelKind::Footnotes), &doc.style)
    })?)?;

    zip.start_file("word/fontTable.xml", options)?;
    zip.write_all(&xml_part(|w| styles::generate_font_table(w, &doc.style))?)?;

    zip.start_file("word/settings.xml", options)?;
    zip.write_all(&xml_part(|w| generate_settings(w, &doc.style))?)?;

    zip.start_file("word/document.xml", options)?;
    zip.write_all(&xml_part(|w| {
        generate_document_xml(w, doc, has_images, &parts)
    })?)?;

    if parts.has(RelKind::Footnotes) {
        zip.start_file("word/footnotes.xml", options)?;
        zip.write_all(&xml_part(|w| generate_footnotes_xml(w, doc))?)?;
    }

    if parts.has(RelKind::Numbering) {
        zip.start_file("word/numbering.xml", options)?;
        zip.write_all(&xml_part(|w| generate_numbering_xml(w, doc))?)?;
    }

    // Write header XML
    if let Some(header) = &doc.header {
        zip.start_file("word/header1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_header_xml(w, header, &doc.style))?)?;
    }

    // Write footer XML — either a PAGE field for page numbering, or static content
    if doc.page_numbering.is_some() {
        zip.start_file("word/footer1.xml", options)?;
        zip.write_all(&xml_part(generate_page_number_footer_xml)?)?;
    } else if let Some(footer) = &doc.footer {
        zip.start_file("word/footer1.xml", options)?;
        zip.write_all(&xml_part(|w| generate_footer_xml(w, footer, &doc.style))?)?;
    }

    // Write image files to word/media/
    for (idx, img) in images.iter().enumerate() {
        let n = idx + 1;
        let ext = img.format.extension();
        let path = format!("word/media/image{n}.{ext}");
        zip.start_file(path, options)?;
        zip.write_all(&img.bytes)?;
    }

    // Always write docProps/core.xml with metadata
    zip.start_file("docProps/core.xml", options)?;
    zip.write_all(&xml_part(|w| generate_core_properties(w, doc))?)?;

    // Custom XML bibliography data source
    if parts.has(RelKind::Bibliography) {
        zip.start_file("customXml/item1.xml", options)?;
        zip.write_all(&xml_part(|w| {
            generate_custom_xml_sources(w, &doc.citation_sources)
        })?)?;

        zip.start_file("customXml/itemProps1.xml", options)?;
        zip.write_all(&xml_part(|w| {
            generate_custom_xml_item_props(w, &doc.citation_sources)
        })?)?;

        zip.start_file("customXml/_rels/item1.xml.rels", options)?;
        zip.write_all(&xml_part(generate_custom_xml_rels)?)?;
    }

    zip.finish()?;
    Ok(())
}

/// Check if the document contains any list items (paragraphs with `list_info` set).
fn doc_has_lists(doc: &Document) -> bool {
    doc.body.elements.iter().any(|el| match el {
        BlockElement::Paragraph(p) => p.list_info.is_some(),
        BlockElement::Table(_) => false,
        BlockElement::BibliographyBlock { paragraphs } => {
            paragraphs.iter().any(|p| p.list_info.is_some())
        }
    })
}
/// A fixed (non-image) relationship part of `document.xml.rels`.
///
/// The order stored in [`DocParts::relationships`] is the **single source of truth**
/// for rId assignment. Images follow the fixed relationships.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RelKind {
    Styles,
    FontTable,
    Footnotes,
    Numbering,
    Settings,
    Header,
    Footer,
    Bibliography,
}

impl RelKind {
    /// The relationship `Type` URL and `Target` filename for this part.
    fn type_and_target(self) -> (String, &'static str) {
        const PREFIX: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
        let (suffix, target) = match self {
            RelKind::Styles => ("styles", "styles.xml"),
            RelKind::FontTable => ("fontTable", "fontTable.xml"),
            RelKind::Footnotes => ("footnotes", "footnotes.xml"),
            RelKind::Numbering => ("numbering", "numbering.xml"),
            RelKind::Settings => ("settings", "settings.xml"),
            RelKind::Header => ("header", "header1.xml"),
            RelKind::Footer => ("footer", "footer1.xml"),
            RelKind::Bibliography => ("customXml", "../customXml/item1.xml"),
        };
        (format!("{PREFIX}{suffix}"), target)
    }
}
