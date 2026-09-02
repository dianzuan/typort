//! Heading semantics and formatting tests.

use crate::common::{fixture_doc_xml, fixture_part, fixture_styles_xml, paragraph_containing};

#[test]
fn issue_show_rule_heading_styles() {
    let xml = fixture_doc_xml("issue_show_rule_heading");
    assert!(
        xml.contains("Main Title"),
        "heading 1 text should be present"
    );
    assert!(
        xml.contains("Blue Subtitle"),
        "heading 2 text should be present"
    );
    // Heading size now lives in the Heading1 style, not on the run.
    let styles = fixture_styles_xml("issue_show_rule_heading");
    assert!(
        styles.contains("w:val=\"36\"") || styles.contains("w:val=\"35\""),
        "Heading1 style should define ~18pt size (36 half-pts). Got:\n{styles}"
    );
    assert!(
        xml.contains("Heading1"),
        "heading 1 should use Heading1 style"
    );
}

#[test]
fn issue_linebreak_in_heading_preserved() {
    let xml = fixture_doc_xml("issue_linebreak_heading");
    assert!(
        xml.contains("Heading with"),
        "heading text before line break should be present"
    );
    assert!(
        xml.contains("line break"),
        "heading text after line break should be present"
    );
    assert!(xml.contains("Heading1"), "should have Heading1 style");
}

#[test]
fn issue_heading_numbering_correct_order() {
    let xml = fixture_doc_xml("issue_heading_numbering_show");
    let intro_pos = xml.find("Introduction").expect("Introduction should exist");
    let bg_pos = xml.find("Background").expect("Background should exist");
    let methods_pos = xml.find("Methods").expect("Methods should exist");
    assert!(
        intro_pos < bg_pos && bg_pos < methods_pos,
        "headings should appear in order: Introduction < Background < Methods"
    );
}

#[test]
fn edge_show_rule_heading_counter_text() {
    let xml = fixture_doc_xml("edge_show_rule_heading_counter");
    for text in [
        "Introduction",
        "Background",
        "Motivation",
        "Methods",
        "Data Collection",
        "Results",
    ] {
        assert!(xml.contains(text), "heading '{text}' should be present");
    }
}

#[test]
fn issue_deep_headings_all_levels() {
    let xml = fixture_doc_xml("issue_deep_headings");
    for text in ["Level 1", "Level 2", "Level 3", "Level 4", "Level 5"] {
        assert!(xml.contains(text), "heading '{text}' should be present");
    }
    assert!(xml.contains("Content under"), "body text should be present");
    for style in ["Heading1", "Heading2", "Heading3", "Heading4", "Heading5"] {
        assert!(xml.contains(style), "style '{style}' should be present");
    }
}

#[test]
fn issue_function_generated_heading() {
    let xml = fixture_doc_xml("issue_function_generated_content");
    assert!(
        xml.contains("Introduction"),
        "first heading should be present"
    );
    assert!(
        xml.contains("Important Result"),
        "function-generated heading should be present"
    );
    assert!(
        xml.contains("Heading1"),
        "level-1 heading style should be present"
    );
    assert!(
        xml.contains("Heading2"),
        "function-generated heading should have Heading2 style"
    );
}

#[test]
fn issue_cjk_heading_numbering_content() {
    let xml = fixture_doc_xml("issue_cjk_heading_numbering");
    assert!(xml.contains("绪论"), "CJK heading text should be present");
    assert!(xml.contains("研究背景"), "CJK subheading should be present");
    assert!(xml.contains("Heading1"), "Heading1 style should be present");
    // The synthesized heading number ("一、") must be emitted in the heading
    // paragraph itself (not merely recovered elsewhere in the document).
    let heading_para = paragraph_containing(&xml, "绪论");
    assert!(
        heading_para.contains("一、"),
        "heading numbering '一、' should be part of the heading paragraph"
    );
}

#[test]
fn heading_smart_quotes_are_resolved() {
    // Regression: smart quotes in a heading were dropped (only body text kept them).
    // See the `edge_heading_smartquotes` fixture.
    let doc_xml = fixture_doc_xml("edge_heading_smartquotes");
    let h1 = paragraph_containing(&doc_xml, "投资于人");
    // Curly opening/closing double quotes (U+201C / U+201D) around the phrase.
    assert!(
        h1.contains('\u{201C}') && h1.contains('\u{201D}'),
        "heading should keep its curly quotes around 投资于人"
    );
    let h2 = paragraph_containing(&doc_xml, "quoted");
    assert!(
        h2.contains('\u{201C}') && h2.contains('\u{201D}'),
        "English quoted heading should keep its curly quotes"
    );
}

#[test]
fn issue_cjk_heading_number_spacing_headings() {
    let xml = fixture_doc_xml("issue_cjk_heading_number_spacing");
    assert!(
        xml.matches("Heading").count() >= 3,
        "should have multiple heading styles"
    );
}

#[test]
fn issue_show_rule_removes_heading_semantics_text() {
    let xml = fixture_doc_xml("issue_show_rule_removes_heading_semantics");
    assert!(
        xml.matches("Heading").count() >= 3,
        "headings should be present despite show rules"
    );
}

#[test]
fn edge_super_sub_in_heading_preserved() {
    let doc_xml = fixture_doc_xml("edge_super_sub_in_heading");

    assert!(
        doc_xml.contains("w:vertAlign w:val=\"subscript\""),
        "heading with H₂O should have subscript vertAlign"
    );
    assert!(
        doc_xml.contains("w:vertAlign w:val=\"superscript\""),
        "heading with x² should have superscript vertAlign"
    );
}

#[test]
fn features_chinese_heading_numbering_definition() {
    let num_xml = fixture_part("complex_paper", "word/numbering.xml");

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
fn heading_run_props_not_redundant_with_style() {
    // A plain heading run must NOT repeat the Heading style's bold/size: the
    // pStyle already carries them, and duplicating them fights a Word template.
    // A genuinely-distinct inline span (italic) inside a heading must survive.
    // See the `heading_redundant_run_props` fixture.
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
