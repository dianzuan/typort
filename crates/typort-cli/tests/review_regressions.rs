//! Regression tests for the findings of the 2026-07 branch review.
//!
//! Each test pins one confirmed defect from that review to its fixture under
//! `tests/fixtures/`. Kept as a separate integration-test file on purpose —
//! CLAUDE.md asks new test areas not to grow `integration.rs` further.

mod common;
use common::{fixture_doc_xml, fixture_part, paragraph_containing};

// ---------------------------------------------------------------------------
// Paged style patches must reach nested-table cell content
// ---------------------------------------------------------------------------

#[test]
fn nested_table_cell_keeps_paged_styles() {
    let doc_xml = fixture_doc_xml("nested_table_cell_style");
    let marker_para = paragraph_containing(&doc_xml, "RED-NESTED-MARKER");
    assert!(
        marker_para.contains("<w:color"),
        "rendering-detected color must survive on a nested-table cell's serialized \
         content, got:\n{marker_para}"
    );
}

// ---------------------------------------------------------------------------
// Coalescing must not merge adjacent forced line breaks
// ---------------------------------------------------------------------------

#[test]
fn consecutive_linebreaks_survive_coalescing() {
    let doc_xml = fixture_doc_xml("consecutive_linebreaks");

    let func_form = paragraph_containing(&doc_xml, "first");
    assert_eq!(
        func_form.matches("<w:br/>").count(),
        2,
        "#linebreak()#linebreak() must emit two <w:br/>s (a blank line), got:\n{func_form}"
    );

    let markup_form = paragraph_containing(&doc_xml, "alpha");
    assert_eq!(
        markup_form.matches("<w:br/>").count(),
        2,
        r"`\ \` must emit two <w:br/>s (a blank line), got:\n{markup_form}"
    );

    // Recovery regression: the short paged lines of a break-split paragraph
    // ("first", "alpha", "beta" are each < 6 chars) must dedup against the
    // paragraph's break segments, not be re-injected as centered orphans.
    for word in ["first", "second", "alpha", "beta"] {
        assert_eq!(
            doc_xml.matches(&format!(">{word}<")).count(),
            1,
            "{word:?} must appear exactly once — a duplicate means recovery re-injected it"
        );
    }
}

// ---------------------------------------------------------------------------
// Recovery body zone must use the document's actual margins
// ---------------------------------------------------------------------------

#[test]
fn small_margin_placed_content_recovered() {
    let doc_xml = fixture_doc_xml("small_margin_placed");
    assert!(
        doc_xml.contains("PAGE-TWO-TOP-BANNER-UNIQUE"),
        "placed content near the edge of a small-margin page must be recovered"
    );
    // The regular body text must not be misfiled as header/footer either.
    assert!(
        doc_xml.contains("First page body text."),
        "body text within the default-margin band must stay in the body"
    );
}

// ---------------------------------------------------------------------------
// Figure rasters are keyed by figure location; <img> content by src data-URL
// ---------------------------------------------------------------------------

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
        "the raster must attach to the drawing figure (after the quote figure), \
         not be stolen by the quote figure"
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

// ---------------------------------------------------------------------------
// Explicit page breaks: span-anchored recovery
// ---------------------------------------------------------------------------

#[test]
fn pagebreak_after_heading_recovered() {
    let doc_xml = fixture_doc_xml("pagebreak_after_heading");
    assert_eq!(
        doc_xml.matches("w:type=\"page\"").count(),
        1,
        "a #pagebreak() following a heading must produce exactly one hard break"
    );
    let break_pos = doc_xml.find("w:type=\"page\"").unwrap();
    assert!(
        doc_xml.find("Part I").unwrap() < break_pos && break_pos < doc_xml.find("Part II").unwrap(),
        "the break must sit between the two headings"
    );
}

#[test]
fn pagebreak_after_repeated_text_not_teleported() {
    let doc_xml = fixture_doc_xml("pagebreak_repeated_text");
    assert_eq!(
        doc_xml.matches("w:type=\"page\"").count(),
        1,
        "exactly one hard break"
    );
    let break_pos = doc_xml.find("w:type=\"page\"").unwrap();
    let second_occurrence = doc_xml.rfind("The same closing line.").unwrap();
    assert!(
        break_pos > second_occurrence,
        "the break must follow the SECOND occurrence of the repeated line, \
         not teleport to the first"
    );
    assert!(
        break_pos < doc_xml.find("Final section.").unwrap(),
        "the break must precede the final section"
    );
}

#[test]
fn pagebreak_inside_include_recovered() {
    let doc_xml = fixture_doc_xml("pagebreak_include");
    assert_eq!(
        doc_xml.matches("w:type=\"page\"").count(),
        1,
        "a #pagebreak() inside an #include'd chapter must be recovered"
    );
    let break_pos = doc_xml.find("w:type=\"page\"").unwrap();
    assert!(
        doc_xml.find("Chapter one text.").unwrap() < break_pos
            && break_pos < doc_xml.find("Chapter two text.").unwrap(),
        "the break must sit between the chapters"
    );
}

// ---------------------------------------------------------------------------
// Table borders must be decided per table, not by a document-global vote
// ---------------------------------------------------------------------------

#[test]
fn table_borders_decided_per_table() {
    let doc_xml = fixture_doc_xml("mixed_table_borders");

    let tables: Vec<&str> = {
        let mut out = Vec::new();
        let mut rest = doc_xml.as_str();
        while let Some(start) = rest.find("<w:tbl>") {
            let end = rest[start..].find("</w:tbl>").map(|e| start + e).unwrap();
            out.push(&rest[start..end]);
            rest = &rest[end..];
        }
        out
    };
    assert_eq!(tables.len(), 2, "fixture has two top-level tables");

    let borderless = tables[0];
    let borders_block = {
        let s = borderless
            .find("<w:tblBorders>")
            .expect("tblBorders present");
        let e = borderless
            .find("</w:tblBorders>")
            .expect("tblBorders closed");
        &borderless[s..e]
    };
    assert!(
        !borders_block.contains("w:val=\"single\""),
        "stroke:none table must not gain invented borders (footnote separator \
         must not vote), got:\n{borders_block}"
    );

    let three_line = tables[1];
    assert!(
        three_line.contains("<w:top w:val=\"single\" w:sz=\"8\"")
            && three_line.contains("<w:bottom w:val=\"single\" w:sz=\"8\""),
        "three-line table keeps its own 1pt outer rules, got:\n{three_line}"
    );
    assert!(
        !three_line.contains("<w:insideV w:val=\"single\""),
        "three-line table must not gain vertical borders"
    );
}

// ---------------------------------------------------------------------------
// A template-drawn #line() must survive as a horizontal rule
// ---------------------------------------------------------------------------

#[test]
fn imported_template_line_rule_recovered() {
    let doc_xml = fixture_doc_xml("imported_line_rule");
    assert!(
        doc_xml.contains("w:pBdr"),
        "a #line() drawn via an imported template must produce a horizontal rule"
    );
}

// ---------------------------------------------------------------------------
// #enum(start: N) must keep its numbering (startOverride from <ol start>)
// ---------------------------------------------------------------------------

#[test]
fn enum_custom_start_keeps_numbering() {
    let numbering = fixture_part("enum_custom_start", "word/numbering.xml");
    assert!(
        numbering.contains("<w:startOverride w:val=\"4\"/>"),
        "#enum(start: 4) must carry startOverride 4, got:\n{numbering}"
    );
    // The first (plain) list still restarts at 1.
    assert!(
        numbering.contains("<w:startOverride w:val=\"1\"/>"),
        "a plain enum must still restart at 1"
    );
}

// ---------------------------------------------------------------------------
// Inline math in list items must become OMML, not leaked MathML glyph text
// ---------------------------------------------------------------------------

#[test]
fn list_inline_math_emits_omml() {
    let doc_xml = fixture_doc_xml("list_inline_math");

    let bullet_item = paragraph_containing(&doc_xml, "item one with");
    assert!(
        bullet_item.contains("<m:oMath>"),
        "bullet item's inline equation must be OMML, got:\n{bullet_item}"
    );
    let enum_item = paragraph_containing(&doc_xml, "numbered with");
    assert!(
        enum_item.contains("<m:oMath>"),
        "enum item's inline equation must be OMML, got:\n{enum_item}"
    );
    // No math-italic glyph may leak into plain <w:t> text.
    assert!(
        !doc_xml.contains("\u{1D465}"), // 𝑥 (math italic x)
        "MathML glyphs must not leak as literal text"
    );
}
