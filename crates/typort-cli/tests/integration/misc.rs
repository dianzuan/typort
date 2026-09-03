//! Tests that don't fit a more specific area.

use crate::common::{
    fixture_dir, fixture_doc_xml, fixture_document, fixture_package, fixture_package_from_document,
    fixture_styles_xml,
};

#[test]
fn italic_text_produces_w_i_element() {
    let doc_xml = fixture_doc_xml("italic_test");

    assert!(
        doc_xml.contains("<w:i/>"),
        "should have italic formatting element <w:i/>"
    );
    assert!(
        doc_xml.contains("emphasized text"),
        "should contain italic text content"
    );
}

#[test]
fn docx_contains_core_properties() {
    let package = fixture_package("hello");

    // Verify docProps/core.xml exists
    let names: Vec<&str> = package.part_names().collect();
    assert!(
        names.contains(&"docProps/core.xml"),
        "docx should contain docProps/core.xml, got: {names:?}"
    );

    // Verify core.xml content
    let core_xml = package.part_text("docProps/core.xml");
    assert!(
        core_xml.contains("cp:coreProperties"),
        "core.xml should have cp:coreProperties root element"
    );
    assert!(
        core_xml.contains("dc:title"),
        "core.xml should contain dc:title element"
    );
    assert!(
        core_xml.contains("Hello World"),
        "dc:title should contain the first heading text"
    );
    assert!(
        core_xml.contains("dcterms:created"),
        "core.xml should contain dcterms:created element"
    );

    // Verify _rels/.rels references core properties
    let rels_xml = package.part_text("_rels/.rels");
    assert!(
        rels_xml.contains("core-properties"),
        "_rels/.rels should reference core-properties"
    );

    // Verify content types include core properties
    let ct_xml = package.part_text("[Content_Types].xml");
    assert!(
        ct_xml.contains("core-properties"),
        "content types should reference core-properties"
    );
}

#[test]
fn metadata_title_extracted_from_first_heading() {
    let doc = fixture_document("hello");

    assert_eq!(
        doc.metadata.title.as_deref(),
        Some("Hello World"),
        "metadata title should be extracted from first heading"
    );
}

#[test]
fn preset_overrides_page_margins() {
    let mut doc = fixture_document("hello");

    // Load the test preset fixture
    let preset = typort_presets::load_preset(&fixture_dir("presets"), "example").unwrap();
    preset.apply(&mut doc);

    // Verify margins were overridden
    assert_eq!(
        doc.page_settings.margin_top, 1440,
        "top margin should be 2.54cm = 1440 twips"
    );
    assert_eq!(
        doc.page_settings.margin_bottom, 1440,
        "bottom margin should be 2.54cm = 1440 twips"
    );
    assert_eq!(
        doc.page_settings.margin_left, 1797,
        "left margin should be 3.17cm = 1797 twips"
    );
    assert_eq!(
        doc.page_settings.margin_right, 1797,
        "right margin should be 3.17cm = 1797 twips"
    );

    // Write and verify margins appear in the XML
    let package = fixture_package_from_document(&doc);
    let doc_xml = package.part_text("word/document.xml");
    assert!(
        doc_xml.contains("w:top=\"1440\""),
        "page margins should reflect preset values"
    );
    assert!(
        doc_xml.contains("w:left=\"1797\""),
        "page margins should reflect preset values"
    );
}

#[test]
fn features_suppress_indent_after_heading() {
    let doc_xml = fixture_doc_xml("complex_paper");

    // First paragraph after heading has firstLine="0" (suppress indent).
    // The Normal style has firstLine="420", so paragraphs after headings should override.
    assert!(
        doc_xml.contains("w:firstLine=\"0\""),
        "document.xml should have firstLine=\"0\" for paragraphs after headings"
    );
}

#[test]
fn doc_title_from_set_document() {
    let package = fixture_package("doc_title");
    let core_xml = package.part_text("docProps/core.xml");

    assert!(
        core_xml.contains("My Custom Title"),
        "core.xml dc:title should be 'My Custom Title', not the first heading. Got: {core_xml}"
    );
    assert!(
        !core_xml.contains("First Heading"),
        "core.xml dc:title should NOT fall back to the first heading when #set document(title:) is used"
    );
}

#[test]
fn doc_author_from_set_document() {
    let package = fixture_package("doc_title");
    let core_xml = package.part_text("docProps/core.xml");

    assert!(
        core_xml.contains("Author Name"),
        "core.xml dc:creator should be 'Author Name'. Got: {core_xml}"
    );
}

#[test]
fn show_rule_colored_bold() {
    let doc_xml = fixture_doc_xml("colored_text");

    // The bold text should have red color.
    // Typst's `red` is #ff4136, not #ff0000.
    assert!(
        doc_xml.contains(r#"<w:color w:val="FF4136"/>"#),
        "red bold text should have w:color FF4136. Got:\n{doc_xml}"
    );
}

#[test]
fn show_rule_bold_size_override() {
    let doc_xml = fixture_doc_xml("show_rule_styles");

    // Bold text should have 14pt = 28 half-points (from show rule: set text(size: 14pt))
    assert!(
        doc_xml.contains(r#"<w:sz w:val="28"/>"#),
        "bold text should have size 28 half-points (14pt) from show rule. Got:\n{doc_xml}"
    );
}

#[test]
fn show_rule_italic_color() {
    let doc_xml = fixture_doc_xml("show_rule_styles");

    // Italic text should be blue (from show rule: set text(fill: rgb("#0000FF")))
    assert!(
        doc_xml.contains(r#"<w:color w:val="0000FF"/>"#),
        "italic text should have blue color from show rule. Got:\n{doc_xml}"
    );
}

/// Regression: font/size detection must be deterministic.
///
/// `doc_title` has one heading line and one body line of equal glyph count,
/// producing a 1:1 tie in the size-frequency map. A bare `max_by_key` over that
/// `HashMap` resolved the tie by iteration order, so the detected body size
/// (and the derived line pitch, and which runs carry an explicit `w:sz`) flipped
/// between runs. The detection now breaks ties deterministically — preferring
/// the smaller size as the body baseline — so output is byte-identical run to
/// run. (Rust's `HashMap` reseeds per instance within a process, so repeated
/// conversion in one test genuinely exercises different iteration orders.)
#[test]
fn doc_title_conversion_is_deterministic() {
    let first = fixture_doc_xml("doc_title");
    for _ in 0..8 {
        assert_eq!(
            fixture_doc_xml("doc_title"),
            first,
            "doc_title output must be byte-identical across runs"
        );
    }
    // Pin the semantically-correct settled value: an 11pt body (w:sz 22 in the
    // Normal style), not the 15.5pt heading size (w:sz 31). (Line spacing is no
    // longer emitted — Pandoc-aligned — so the body size is pinned directly.)
    let styles = fixture_styles_xml("doc_title");
    let normal_start = styles
        .find(r#"w:styleId="Normal""#)
        .expect("Normal style present");
    let normal = &styles[normal_start
        ..styles[normal_start..]
            .find("</w:style>")
            .map_or(styles.len(), |e| normal_start + e)];
    assert!(
        normal.contains(r#"w:sz w:val="22""#),
        "body should be detected as 11pt (Normal w:sz=22): {normal}"
    );
}

/// Regression (page.rs scope-aware source-AST set-rule collection): the global
/// body size must come from the `#show:` template closure's `set text(size:)`
/// (12pt), not from a `set text(size: 9pt)` buried in a non-template helper
/// closure or a nested `#block[…]`. Before the fix, `collect_set_rules` was
/// scope-blind and first-wins, so the 9pt helper/block size clobbered the real
/// global 12pt.
#[test]
fn body_size_from_show_template_not_nested_block() {
    let styles = fixture_styles_xml("edge_body_size_show_template");
    let normal_start = styles
        .find(r#"w:styleId="Normal""#)
        .expect("Normal style present");
    let normal = &styles[normal_start
        ..styles[normal_start..]
            .find("</w:style>")
            .map_or(styles.len(), |e| normal_start + e)];
    assert!(
        normal.contains(r#"w:sz w:val="24""#),
        "global body size must be the show-template 12pt (Normal w:sz=24), \
         not the 9pt helper/block size: {normal}"
    );
}

#[test]
fn issue_today_is_real_date_not_fixed() {
    // World::today() must return the real current date, not a fixed placeholder.
    // Compare against the system date computed the same way at test time, so the
    // assertion holds on any day. Falls back to UTC if the local zone is
    // unavailable — matching the implementation in world.rs.
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let expected = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month() as u8,
        now.day()
    );
    let xml = fixture_doc_xml("issue_today_real_date");
    assert!(
        xml.contains(&expected),
        "document should contain today's date {expected}, but did not (fixed placeholder?)"
    );
}

#[test]
fn edge_empty_paragraphs_no_crash() {
    let xml = fixture_doc_xml("edge_empty_paragraphs");
    assert!(
        xml.contains("First paragraph"),
        "first text should be present"
    );
    assert!(
        xml.contains("Third paragraph"),
        "third text should be present"
    );
    assert!(
        xml.contains("Last paragraph"),
        "last text should be present"
    );
}

#[test]
fn edge_theorem_proof_content() {
    let xml = fixture_doc_xml("edge_theorem_proof");
    for text in [
        "Continuity",
        "Intermediate Value",
        "Theorem",
        "Proof",
        "Definition",
        "bounded monotone",
        "Preliminaries",
    ] {
        assert!(xml.contains(text), "'{text}' should be present");
    }
    assert!(
        xml.contains("<m:oMathPara>"),
        "math in definition should produce OMML"
    );
}

#[test]
fn issue_custom_doc_properties_metadata() {
    let xml = fixture_doc_xml("issue_custom_doc_properties");
    assert!(
        xml.contains("Abstract"),
        "abstract heading should be present"
    );
    assert!(
        xml.contains("Introduction"),
        "intro heading should be present"
    );
}

#[test]
fn issue_metadata_case_dedup() {
    let xml = fixture_doc_xml("issue_metadata_case_dedup");
    assert!(xml.contains("test document"), "document body text present");
}
