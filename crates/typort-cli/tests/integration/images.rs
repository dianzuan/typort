//! Image embedding and figure-caption tests.

use crate::common;
use crate::common::{fixture_doc_xml, fixture_document, fixture_package};

#[test]
fn image_embeds_in_docx() {
    let package = fixture_package("image_test");

    // Check image file exists in ZIP
    let names: Vec<&str> = package.part_names().collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "should have image in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = package.part_text("word/document.xml");
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
    let ct_xml = package.part_text("[Content_Types].xml");
    assert!(
        ct_xml.contains("image/png"),
        "content types should include image/png"
    );
}

#[test]
fn text_figure_does_not_steal_drawing_raster() {
    let doc_xml = fixture_doc_xml("figure_kinds_no_steal");
    assert!(
        doc_xml.contains("QUOTE-BODY-SENTENCE"),
        "the quote figure's body must stay editable text, not be replaced by an image"
    );
    assert_eq!(
        doc_xml.matches("<w:drawing").count(),
        1,
        "exactly one embedded image: the drawing figure's own canvas"
    );
    let quote_caption = doc_xml.find("A quote figure").unwrap();
    let drawing_pos = doc_xml.find("<w:drawing").unwrap();
    assert!(
        drawing_pos > quote_caption,
        "the raster must attach to the drawing figure, not be stolen by the quote figure"
    );
}

#[test]
fn image_inside_rounded_container_embedded() {
    let doc_xml = fixture_doc_xml("rounded_container_image");
    assert_eq!(
        doc_xml.matches("<w:drawing").count(),
        2,
        "both the rounded-container image and the figure image must embed"
    );
    let container_img = doc_xml.find("<w:drawing").unwrap();
    assert!(
        container_img < doc_xml.find("A real figure").unwrap(),
        "the container's image must appear before the figure"
    );
}

#[test]
fn image_has_relationships_in_rels() {
    let package = fixture_package("image_test");

    // Check document rels include image relationship
    let rels_xml = package.part_text("word/_rels/document.xml.rels");
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

    let doc = fixture_document("image_test");

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

    let doc = fixture_document("image_test");

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
    let package = fixture_package("svg_test");

    // Check image file exists in ZIP
    let names: Vec<&str> = package.part_names().collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "SVG should be rasterized to PNG and embedded in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = package.part_text("word/document.xml");
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

    let doc = fixture_document("svg_test");

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
    let package = fixture_package("edge_image_format_mix");
    let names: Vec<&str> = package.part_names().collect();
    let media = names
        .iter()
        .filter(|n| n.starts_with("word/media/"))
        .count();
    assert_eq!(
        media, 3,
        "all three figure images must be embedded (the GIF re-encoded), got {media}: {names:?}"
    );
}

#[test]
fn issue_figure_caption_present() {
    let xml = fixture_doc_xml("issue_figure_caption");
    assert!(
        xml.contains("Demographics of participants"),
        "first figure caption text should be present"
    );
    assert!(
        xml.contains("Fruit prices at the market"),
        "second figure caption text should be present"
    );
    assert!(
        xml.contains("Table") && xml.contains("1"),
        "caption should include 'Table 1' numbering"
    );
}

#[test]
fn issue_show_caption_inplace_keeps_document_order() {
    // A custom `show figure.caption: it => [...]` rule makes Typst HTML export emit
    // each caption as a nested inline `caption` Tag inside the figure body `<p>`
    // (not a `<figcaption>` element). Without a "caption" arm in handle_inline_tag,
    // the captions were find_tag_end-skipped, then re-scraped from Paged geometry by
    // recovery and hoisted/merged to the top — torn from their figures and out of
    // order. The "caption" arm emits each caption as its own block in document order.
    let xml = fixture_doc_xml("issue_show_caption_inplace");

    // Document-order positions of the body prose and the two captions. `str::find`
    // returns the byte offset of the first occurrence, i.e. document order in the XML.
    let before = xml
        .find("must remain first")
        .expect("intro body paragraph present");
    let alpha = xml
        .find("Caption alpha one")
        .expect("first caption text present");
    let between = xml
        .find("must keep its slot")
        .expect("inter-figure body paragraph present");
    let beta = xml
        .find("Caption beta two")
        .expect("second caption text present");
    let after = xml
        .find("near the end")
        .expect("trailing body paragraph present");

    // Captions must stay interleaved with their surrounding prose, not hoisted.
    assert!(
        before < alpha && alpha < between,
        "first caption must sit between the intro and the inter-figure paragraph \
         (before={before}, alpha={alpha}, between={between})"
    );
    assert!(
        between < beta && beta < after,
        "second caption must sit between the inter-figure and trailing paragraph \
         (between={between}, beta={beta}, after={after})"
    );

    // And recovery must not duplicate the now-in-place captions.
    assert_eq!(
        xml.matches("Caption alpha one").count(),
        1,
        "first caption should appear exactly once (recovery duplicate?)"
    );
    assert_eq!(
        xml.matches("Caption beta two").count(),
        1,
        "second caption should appear exactly once (recovery duplicate?)"
    );
}

#[test]
fn edge_figure_placement_tables_and_refs() {
    let xml = fixture_doc_xml("edge_figure_placement");
    assert!(
        xml.contains("Performance comparison"),
        "first figure caption should be present"
    );
    assert!(
        xml.contains("Hyperparameters"),
        "second figure caption should be present"
    );
    assert!(
        xml.contains("<w:tbl>"),
        "tables in figures should be present"
    );
    assert!(xml.contains("Heading1"), "heading styles should be present");
}

#[test]
fn edge_subfigures_content() {
    let xml = fixture_doc_xml("edge_subfigures");
    assert!(
        xml.contains("Subfigure placeholder"),
        "subfigure placeholders should be present"
    );
    assert!(
        xml.contains("Comparison of two methods"),
        "main figure caption should be present"
    );
    assert!(
        xml.contains("training data"),
        "side-by-side table caption should be present"
    );
}

#[test]
fn issue_table_figure_caption_text() {
    let xml = fixture_doc_xml("issue_table_figure_caption");
    assert!(
        xml.contains("First table with a caption"),
        "first caption should be present"
    );
    assert!(
        xml.contains("Second table with a caption"),
        "second caption should be present"
    );
    assert!(xml.contains("<w:tbl>"), "tables should be present");
}

#[test]
fn issue_list_of_figures_toc() {
    let xml = fixture_doc_xml("issue_list_of_figures");
    assert!(xml.contains("TOC"), "TOC field code should be present");
    assert!(xml.contains("Introduction"), "heading should be present");
    assert!(
        xml.contains("blue rectangle"),
        "figure caption should be present"
    );
    assert!(
        xml.contains("simple table"),
        "table caption should be present"
    );
}

#[test]
fn issue_figure_counter_reset_tables() {
    let xml = fixture_doc_xml("issue_figure_counter_reset");
    assert!(
        xml.contains("First table"),
        "first caption should be present"
    );
    assert!(
        xml.contains("Table after reset"),
        "reset caption should be present"
    );
    let table_count = xml.matches("<w:tbl>").count();
    assert!(
        table_count >= 2,
        "should have at least 2 tables, got {table_count}"
    );
}

#[test]
fn issue_caption_prefix_custom_supplement() {
    let xml = fixture_doc_xml("issue_caption_prefix_custom");
    assert!(
        xml.contains("Sample data"),
        "first caption should be present"
    );
    assert!(
        xml.contains("A diagram"),
        "second caption should be present"
    );
    assert!(xml.contains("表"), "Chinese supplement should be present");
    assert!(xml.contains("Fig."), "custom supplement should be present");
}
