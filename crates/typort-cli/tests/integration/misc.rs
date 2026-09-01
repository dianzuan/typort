//! Tests that don't fit a more specific area.

use crate::common::{fixture_doc_xml, fixture_styles_xml};
use std::io::Cursor;
use std::path::Path;

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
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Verify docProps/core.xml exists
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n == "docProps/core.xml"),
        "docx should contain docProps/core.xml, got: {names:?}"
    );

    // Verify core.xml content
    let core_xml = std::io::read_to_string(reader.by_name("docProps/core.xml").unwrap()).unwrap();
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
    let rels_xml = std::io::read_to_string(reader.by_name("_rels/.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("core-properties"),
        "_rels/.rels should reference core-properties"
    );

    // Verify content types include core properties
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("core-properties"),
        "content types should reference core-properties"
    );
}

#[test]
fn metadata_title_extracted_from_first_heading() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    assert_eq!(
        doc.metadata.title.as_deref(),
        Some("Hello World"),
        "metadata title should be extracted from first heading"
    );
}

#[test]
fn preset_overrides_page_margins() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let mut doc = typort_core::convert::convert(&world).unwrap();

    // Load the test preset fixture
    let preset =
        typort_presets::load_preset(Path::new("../../tests/fixtures/presets"), "example").unwrap();

    // Apply preset page margins
    if let Some(page) = &preset.page {
        if let Some(top) = page.margin_top_cm {
            doc.page_settings.margin_top = typort_presets::cm_to_twips(top);
        }
        if let Some(bottom) = page.margin_bottom_cm {
            doc.page_settings.margin_bottom = typort_presets::cm_to_twips(bottom);
        }
        if let Some(left) = page.margin_left_cm {
            doc.page_settings.margin_left = typort_presets::cm_to_twips(left);
        }
        if let Some(right) = page.margin_right_cm {
            doc.page_settings.margin_right = typort_presets::cm_to_twips(right);
        }
    }

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
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
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
fn features_footnote_restart_and_font_hint() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Feature 1: Footnote per-page restart (matching Typst's default numbering)
    let settings_xml =
        std::io::read_to_string(reader.by_name("word/settings.xml").unwrap()).unwrap();
    assert!(
        settings_xml.contains("w:footnotePr"),
        "settings.xml should contain w:footnotePr"
    );
    assert!(
        settings_xml.contains("eachPage"),
        "settings.xml should restart numbering each page"
    );

    // Feature 1: sectPr also has footnote properties
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    // The sectPr should contain footnotePr
    let sect_pr_pos = doc_xml.find("w:sectPr").expect("should have sectPr");
    let after_sect = &doc_xml[sect_pr_pos..];
    assert!(
        after_sect.contains("w:footnotePr"),
        "sectPr should contain w:footnotePr for per-section footnote restart"
    );

    // Feature 9: East Asian font hint
    let styles_xml = std::io::read_to_string(reader.by_name("word/styles.xml").unwrap()).unwrap();
    assert!(
        styles_xml.contains("w:hint=\"eastAsia\""),
        "styles.xml should contain w:hint=\"eastAsia\" for proper font selection"
    );
}

#[test]
fn footnote_circled_numbering_format_emitted() {
    // Regression (typst 0.15 migration): the `doc-noteref` role moved ONTO the
    // `<sup>` element (whose child is `<a>N`), so detect_footnote_format's old
    // `<sup>`-child scan never matched and the circled (①②③) footnote numbering
    // format was silently dropped — Word then rendered ①②③ as 1,2,3. fn25_test
    // declares `#set footnote(numbering: "①")`.
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/fn25_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let settings_xml =
        std::io::read_to_string(reader.by_name("word/settings.xml").unwrap()).unwrap();
    assert!(
        settings_xml.contains("decimalEnclosedCircle"),
        "circled footnote numbering must emit w:numFmt=decimalEnclosedCircle, got: {settings_xml}"
    );
}

#[test]
fn footnote_math_not_duplicated() {
    // Regression (typst 0.15 migration): collect_footnote_inlines descended into
    // the new native MathML `<math>` element and emitted the equation a SECOND
    // time as literal Mathematical-Alphanumeric glyphs in a body `<w:t>` run, on
    // top of the correct OMML. Body runs must carry no math-script glyphs (those
    // belong only in OMML `<m:t>`).
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_math_in_footnote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();
    assert!(
        fn_xml.contains("<m:oMath>"),
        "footnote math should be present as OMML, got: {fn_xml}"
    );
    let mut rest = fn_xml.as_str();
    let mut leaked = false;
    while let Some(pos) = rest.find("<w:t") {
        rest = &rest[pos + 4..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let head = &rest[..open_end];
        // a body text run is `<w:t>` or `<w:t attr…>`, not `<w:tab/>`
        if (head.is_empty() || head.starts_with(' '))
            && let Some(close) = rest[open_end + 1..].find("</w:t>")
            && rest[open_end + 1..open_end + 1 + close]
                .chars()
                .any(|c| ('\u{1D400}'..='\u{1D7FF}').contains(&c))
        {
            leaked = true;
            break;
        }
        rest = &rest[open_end..];
    }
    assert!(
        !leaked,
        "footnote math must not be duplicated as literal glyphs in a <w:t> run: {fn_xml}"
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
fn features_table_width_percentage() {
    let doc_xml = fixture_doc_xml("complex_paper");

    // Feature 6: Table uses percentage width (100%)
    assert!(
        doc_xml.contains("<w:tblW w:w=\"5000\" w:type=\"pct\"/>"),
        "table should have 100% width via pct type"
    );
    // Feature 6: Cells have width defined
    assert!(
        doc_xml.contains("w:tcW"),
        "table cells should have w:tcW width elements"
    );
}

#[test]
fn features_chinese_heading_numbering_definition() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let num_xml = std::io::read_to_string(reader.by_name("word/numbering.xml").unwrap()).unwrap();

    // Feature 2: Chinese heading numbering abstract definition exists
    assert!(
        num_xml.contains("chineseCountingThousand"),
        "numbering.xml should contain chineseCountingThousand format"
    );
    assert!(
        num_xml.contains("decimalEnclosedCircleChinese"),
        "numbering.xml should contain decimalEnclosedCircleChinese for level 4"
    );
    assert!(
        num_xml.contains("w:abstractNumId=\"3\""),
        "numbering.xml should have abstractNumId 3 for Chinese headings"
    );
    assert!(
        num_xml.contains("w:numId=\"3\""),
        "numbering.xml should have numId 3 instance for Chinese headings"
    );
}

#[test]
fn doc_title_from_set_document() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/doc_title.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let core_xml = std::io::read_to_string(reader.by_name("docProps/core.xml").unwrap()).unwrap();

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/doc_title.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let core_xml = std::io::read_to_string(reader.by_name("docProps/core.xml").unwrap()).unwrap();

    assert!(
        core_xml.contains("Author Name"),
        "core.xml dc:creator should be 'Author Name'. Got: {core_xml}"
    );
}

#[test]
fn show_rule_heading_centered() {
    let doc_xml = fixture_doc_xml("centered_heading");

    // The heading should be detected as centered from the PagedDocument
    // Look for a Heading1 paragraph with center alignment
    assert!(
        doc_xml.contains(r#"<w:jc w:val="center"/>"#),
        "centered heading should have w:jc center. Got:\n{doc_xml}"
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
fn large_title_not_split_by_tabs() {
    let doc_xml = fixture_doc_xml("large_title_test");

    // The large centered title "大标题测试文档" should NOT be split by tab characters.
    // Previously, large CJK characters at 22pt exceeded the x-cluster gap threshold,
    // causing each character to be treated as a separate column.
    assert!(
        !doc_xml.contains("<w:tab/>"),
        "large title should not contain tab separators. Got:\n{doc_xml}"
    );

    // The title characters should all be present
    assert!(
        doc_xml.contains('大') && doc_xml.contains('标') && doc_xml.contains('文'),
        "title characters should be in the output"
    );
}

#[test]
fn show_rule_heading_font_and_size() {
    let doc_xml = fixture_doc_xml("show_rule_styles");

    // Heading should be centered (from show rule: align(center))
    assert!(
        doc_xml.contains(r#"<w:jc w:val="center"/>"#),
        "heading should be centered via show rule. Got:\n{doc_xml}"
    );

    // Heading font should be overridden to DejaVu Sans (from show rule)
    assert!(
        doc_xml.contains("DejaVu Sans"),
        "heading should use DejaVu Sans font from show rule. Got:\n{doc_xml}"
    );

    // Heading size 18pt = 36 half-points now lives in the Heading1 STYLE (the
    // run inherits it), not as a redundant per-run <w:sz>.
    let styles_xml = fixture_styles_xml("show_rule_styles");
    assert!(
        styles_xml.contains(r#"<w:sz w:val="36"/>"#),
        "Heading1 style should define size 36 half-points (18pt). Got:\n{styles_xml}"
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
fn heading_run_props_not_redundant_with_style() {
    // A plain heading run must NOT repeat the Heading style's bold/size: the
    // pStyle already carries them, and duplicating them fights a Word template.
    // A genuinely-distinct inline span (italic) inside a heading must survive.
    // See tests/fixtures/heading_redundant_run_props.typ.
    let doc_xml = fixture_doc_xml("heading_redundant_run_props");

    // Isolate the plain heading paragraph; its run should carry no redundant
    // <w:b/>/<w:sz> (those live in the Heading1 style).
    let plain = doc_xml
        .split("<w:p>")
        .find(|p| p.contains("Plain Heading One"))
        .expect("plain heading paragraph present");
    assert!(
        plain.contains(r#"<w:pStyle w:val="Heading1"/>"#),
        "plain heading should carry Heading1 pStyle. Got:\n{plain}"
    );
    assert!(
        !plain.contains("<w:b/>"),
        "plain heading run must not repeat the style's <w:b/>. Got:\n{plain}"
    );
    assert!(
        !plain.contains("<w:sz "),
        "plain heading run must not repeat the style's <w:sz>. Got:\n{plain}"
    );

    // The italic span inside the second heading must keep its distinct override.
    let styled = doc_xml
        .split("<w:p>")
        .find(|p| p.contains("Italic"))
        .expect("styled heading paragraph present");
    assert!(
        styled.contains("<w:i/>"),
        "italic span inside a heading must keep <w:i/>. Got:\n{styled}"
    );
}
