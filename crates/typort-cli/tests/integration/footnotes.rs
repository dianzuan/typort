//! Footnote and endnote tests.

use crate::common::{fixture_doc_xml, fixture_document, fixture_package, fixture_styles_xml};

#[test]
fn issue_blockquote_in_footnote_content() {
    let xml = fixture_doc_xml("issue_blockquote_in_footnote");
    assert!(
        xml.contains("w:footnoteReference"),
        "footnote reference should be present"
    );
    assert!(
        xml.contains("Another paragraph"),
        "body text should be present"
    );
}

#[test]
fn issue_footnote_with_link_refs() {
    let xml = fixture_doc_xml("issue_footnote_with_link");
    assert!(xml.contains("Introduction"), "heading should be present");
    assert!(
        xml.contains("Conclusion"),
        "second heading should be present"
    );
    let fn_count = xml.matches("w:footnoteReference").count();
    assert!(
        fn_count >= 2,
        "should have at least 2 footnotes, got {fn_count}"
    );
    assert!(
        xml.matches("w:bookmarkStart").count() >= 2,
        "cross-references should produce bookmarks"
    );
}

#[test]
fn issue_footnote_separator_has_refs() {
    let xml = fixture_doc_xml("issue_footnote_separator");
    let fn_count = xml.matches("w:footnoteReference").count();
    assert_eq!(
        fn_count, 3,
        "should have 3 footnote references, got {fn_count}"
    );
}

#[test]
fn issue_footnote_math_formatting_content() {
    let xml = fixture_doc_xml("issue_footnote_math_formatting");
    let fn_count = xml.matches("w:footnoteReference").count();
    assert_eq!(fn_count, 3, "should have 3 footnotes, got {fn_count}");
}

#[test]
fn issue_endnote_vs_footnote_refs() {
    let xml = fixture_doc_xml("issue_endnote_vs_footnote");
    let fn_count = xml.matches("w:footnoteReference").count();
    assert!(
        fn_count >= 4,
        "should have at least 4 footnote references, got {fn_count}"
    );
}

#[test]
fn issue_footnote_tab_format() {
    let xml = fixture_doc_xml("issue_footnote_tab_format");
    assert!(
        xml.contains("footnote"),
        "footnote reference should be in document"
    );
}

#[test]
fn issue_footnote_in_heading_toc() {
    let xml = fixture_doc_xml("issue_footnote_in_heading_toc");
    let heading_count = xml.matches("Heading1").count();
    assert!(
        heading_count >= 3,
        "should have 3 Heading1 styles, got {heading_count}"
    );
    assert!(xml.contains("TOC"), "should have TOC field");
    assert!(xml.contains("Introduction"), "first heading text present");
    assert!(xml.contains("Results"), "third heading text present");
}

#[test]
fn formatted_footnote_preserves_bold_and_italic() {
    let doc = fixture_document("formatted_footnote");

    // Check the footnote content runs preserve formatting
    assert!(
        !doc.footnotes.is_empty(),
        "should have at least one footnote"
    );
    let fn_content = &doc.footnotes[0].content;
    let has_bold = fn_content
        .iter()
        .any(|i| matches!(i, typort_ooxml::document::InlineElement::Text(r) if r.bold));
    let has_italic = fn_content
        .iter()
        .any(|i| matches!(i, typort_ooxml::document::InlineElement::Text(r) if r.italic));
    assert!(has_bold, "footnote content should have a bold run");
    assert!(has_italic, "footnote content should have an italic run");
}

#[test]
fn formatted_footnote_xml_has_formatting_elements() {
    let package = fixture_package("formatted_footnote");
    let fn_xml = package.part_text("word/footnotes.xml");

    assert!(
        fn_xml.contains("<w:b/>"),
        "footnotes.xml should contain <w:b/> for bold formatting"
    );
    assert!(
        fn_xml.contains("<w:i/>"),
        "footnotes.xml should contain <w:i/> for italic formatting"
    );
}

#[test]
fn footnote_text_size_is_body_size_not_marker_size() {
    // Regression for detect_footnote_size (page.rs): it took the global-minimum
    // small size, which is the superscript reference/marker size (~6.5pt), and
    // pinned FootnoteText to it. The fix measures the footnote BODY runs from the
    // Paged render (located by the semantic footnote content), giving the real
    // footnote text size (~9pt). See the `edge_footnote_size_not_marker` fixture.
    let styles = fixture_styles_xml("edge_footnote_size_not_marker");
    let block = styles
        .split(r#"w:styleId="FootnoteText""#)
        .nth(1)
        .expect("FootnoteText style present");
    let block = block.split("</w:style>").next().unwrap();
    let sz: u32 = block
        .split(r#"<w:sz w:val=""#)
        .nth(1)
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .expect("FootnoteText sz present");
    // Body is 10.5pt (sz 21); the footnote body renders ~9pt (sz 16-20); the
    // superscript marker is ~6.5pt (sz 13). The footnote style must take the
    // footnote-body size, not the marker size.
    assert!(
        (16..21).contains(&sz),
        "FootnoteText size must be the footnote body size (~9pt, sz 16-20), not the \
         superscript marker size (~6.5pt, sz 13); got sz={sz}"
    );
}

#[test]
fn features_footnote_restart_and_font_hint() {
    let package = fixture_package("complex_paper");

    // Feature 1: Footnote per-page restart (matching Typst's default numbering)
    let settings_xml = package.part_text("word/settings.xml");
    assert!(
        settings_xml.contains("w:footnotePr"),
        "settings.xml should contain w:footnotePr"
    );
    assert!(
        settings_xml.contains("eachPage"),
        "settings.xml should restart numbering each page"
    );

    // Feature 1: sectPr also has footnote properties
    let doc_xml = package.part_text("word/document.xml");
    // The sectPr should contain footnotePr
    let sect_pr_pos = doc_xml.find("w:sectPr").expect("should have sectPr");
    let after_sect = &doc_xml[sect_pr_pos..];
    assert!(
        after_sect.contains("w:footnotePr"),
        "sectPr should contain w:footnotePr for per-section footnote restart"
    );

    // Feature 9: East Asian font hint
    let styles_xml = package.part_text("word/styles.xml");
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
    let package = fixture_package("fn25_test");
    let settings_xml = package.part_text("word/settings.xml");
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
    let package = fixture_package("edge_math_in_footnote");
    let fn_xml = package.part_text("word/footnotes.xml");
    assert!(
        fn_xml.contains("<m:oMath>"),
        "footnote math should be present as OMML, got: {fn_xml}"
    );
    let mut rest = fn_xml;
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
fn edge_math_in_footnote_preserved() {
    let package = fixture_package("edge_math_in_footnote");
    let fn_xml = package.part_text("word/footnotes.xml");

    assert!(
        fn_xml.contains("m:oMath"),
        "footnotes.xml should contain m:oMath elements for math in footnotes"
    );
}

#[test]
fn smoke_style_footnotes() {
    let package = fixture_package("style_footnotes");
    assert!(
        package.byte_len() > 100,
        "docx output should be non-trivial"
    );
}
