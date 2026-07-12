//! Image embedding and figure-caption tests.

use crate::common;
use crate::common::fixture_doc_xml;
use std::io::Cursor;
use std::path::Path;

#[test]
fn image_embeds_in_docx() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check image file exists in ZIP
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "should have image in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:drawing"),
        "should have w:drawing element"
    );
    assert!(
        doc_xml.contains("wp:inline"),
        "should have wp:inline element"
    );
    assert!(doc_xml.contains("a:blip"), "should have a:blip element");

    // Check content types include image
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("image/png"),
        "content types should include image/png"
    );
}

#[test]
fn image_has_relationships_in_rels() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check document rels include image relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("relationships/image"),
        "document rels should include image relationship"
    );
    assert!(
        rels_xml.contains("media/image1"),
        "document rels should reference media/image1"
    );
}

#[test]
fn image_document_model_has_image_inline() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with Image inlines
    let has_image = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineElement::Image(_)))
        } else {
            false
        }
    });
    assert!(
        has_image,
        "document model should have at least one Image inline element"
    );
}

#[test]
fn image_has_nonzero_emu_dimensions() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    for e in &doc.body.elements {
        if let BlockElement::Paragraph(p) = e {
            for inline in &p.inlines {
                if let InlineElement::Image(img) = inline {
                    assert!(img.width_emu > 0, "image width_emu should be > 0");
                    assert!(img.height_emu > 0, "image height_emu should be > 0");
                    assert!(!img.bytes.is_empty(), "image bytes should not be empty");
                }
            }
        }
    }
}

#[test]
fn svg_image_rasterized_and_embedded() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/svg_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check image file exists in ZIP
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "SVG should be rasterized to PNG and embedded in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:drawing"),
        "SVG image should produce w:drawing element"
    );
    assert!(
        doc_xml.contains("a:blip"),
        "SVG image should produce a:blip element"
    );
}

#[test]
fn svg_image_has_nonzero_dimensions() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/svg_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut found = false;
    for e in &doc.body.elements {
        if let BlockElement::Paragraph(p) = e {
            for inline in &p.inlines {
                if let InlineElement::Image(img) = inline {
                    assert!(img.width_emu > 0, "SVG image width_emu should be > 0");
                    assert!(img.height_emu > 0, "SVG image height_emu should be > 0");
                    assert!(!img.bytes.is_empty(), "SVG image bytes should not be empty");
                    found = true;
                }
            }
        }
    }
    assert!(
        found,
        "should have found at least one image from SVG rasterization"
    );
}

#[test]
fn figure_caption_is_single_paragraph() {
    let doc_xml = fixture_doc_xml("figure_caption");

    // Count how many paragraphs contain parts of the caption text.
    // The caption "Table 1: A simple data table" should NOT be split into
    // separate paragraphs like "Table", "1", ":", "A simple data table".
    // Instead it should be a single paragraph containing all parts.

    // Split document XML on paragraph boundaries to inspect per-paragraph content
    let para_texts: Vec<String> = common::paragraph_texts(&doc_xml);

    // Find paragraphs that contain caption-related text
    let caption_paras: Vec<&String> = para_texts
        .iter()
        .filter(|t| t.contains("A simple data table"))
        .collect();

    assert!(
        !caption_paras.is_empty(),
        "should find at least one paragraph with caption text 'A simple data table', got paragraphs: {para_texts:?}"
    );

    // The key assertion: the paragraph containing "A simple data table"
    // should also contain "Table" (the figure kind prefix), proving they
    // are combined into a single paragraph, not split apart.
    let combined = caption_paras
        .iter()
        .any(|t| t.contains("Table") && t.contains("A simple data table"));
    assert!(
        combined,
        "caption text and figure prefix should be in the same paragraph, but got separate paragraphs: {caption_paras:?}"
    );
}

/// Regression (figure rasterization + recovery.rs): a vector-drawing figure
/// (Bézier curve + an inner text label) must rasterize to a single embedded
/// image with its label baked into the pixels — not recovered into the body as
/// a stray paragraph (the label is absent from the HTML and would otherwise be
/// pulled from the paged output). A sibling table figure must stay an editable
/// table, and both captions must survive.
#[test]
fn drawing_figure_is_rasterized_not_leaked() {
    let xml = fixture_doc_xml("edge_figure_rasterized");
    assert_eq!(
        xml.matches("<w:drawing").count(),
        1,
        "the curve figure should be exactly one embedded image: {xml}"
    );
    assert_eq!(
        xml.matches("<w:tbl>").count(),
        1,
        "the table figure must stay an editable table, not be rasterized: {xml}"
    );
    assert!(
        !xml.contains("ZZLABELZZ"),
        "the canvas label must be baked into the image, not leaked as body text: {xml}"
    );
    assert!(
        xml.contains("Drawn figure") && xml.contains("Real table"),
        "both figure captions must survive: {xml}"
    );
}

#[test]
fn mixed_image_formats_stay_aligned() {
    // A GIF (unsupported raster) between two SVGs must not desync the image FIFO:
    // dropping it used to shift every later image onto the wrong caption. Re-encoding
    // it to PNG keeps all three figures' images. See edge_image_format_mix.typ.
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_image_format_mix.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<String> = (0..reader.len())
        .map(|i| reader.by_index(i).unwrap().name().to_string())
        .collect();
    let media = names
        .iter()
        .filter(|n| n.starts_with("word/media/"))
        .count();
    assert_eq!(
        media, 3,
        "all three figure images must be embedded (the GIF re-encoded), got {media}: {names:?}"
    );
}
