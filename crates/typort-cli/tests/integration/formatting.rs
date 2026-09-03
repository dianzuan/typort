//! Inline formatting, indentation, and run-coalescing tests.

use crate::common;
use crate::common::{
    fixture_doc_xml, fixture_document, fixture_package, fixture_styles_xml, paragraph_containing,
};

#[test]
fn inline_super_produces_text() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    assert!(
        doc_xml.contains("superscript"),
        "document.xml should contain the text 'superscript'"
    );
    assert!(
        doc_xml.contains("w:val=\"superscript\""),
        "superscript text should have w:vertAlign with val=superscript"
    );
}

#[test]
fn inline_sub_produces_text() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    assert!(
        doc_xml.contains("subscript"),
        "document.xml should contain the text 'subscript'"
    );
    assert!(
        doc_xml.contains("w:val=\"subscript\""),
        "subscript text should have w:vertAlign with val=subscript"
    );
}

#[test]
fn inline_underline_produces_text() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    assert!(
        doc_xml.contains("underlined"),
        "document.xml should contain the text 'underlined'"
    );
    assert!(
        doc_xml.contains("<w:u w:val=\"single\"/>"),
        "underlined text should have w:u with val=single"
    );
}

#[test]
fn inline_strike_produces_text() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    assert!(
        doc_xml.contains("strikethrough"),
        "document.xml should contain the text 'strikethrough'"
    );
    assert!(
        doc_xml.contains("<w:strike/>"),
        "strikethrough text should have w:strike element"
    );
}

#[test]
fn inline_raw_produces_monospace() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    // Either `inline code` (backtick syntax) or #raw("raw text") should produce monospace
    assert!(
        doc_xml.contains("inline code") || doc_xml.contains("raw text"),
        "document.xml should contain 'inline code' or 'raw text'"
    );
    assert!(
        doc_xml.contains("w:rFonts"),
        "raw/code text should have a font override (w:rFonts)"
    );
}

#[test]
fn inline_highlight_produces_text() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    assert!(
        doc_xml.contains("highlighted"),
        "document.xml should contain the text 'highlighted'"
    );
    assert!(
        doc_xml.contains("<w:highlight w:val=\"yellow\"/>"),
        "highlighted text should have w:highlight with val=yellow"
    );
}

#[test]
fn inline_smallcaps_text_preserved() {
    let doc_xml = common::fixture_doc_xml("inline_formatting");
    // SmallcapsElem doesn't have the Tagged trait in Typst 0.15, so it won't
    // produce Tag::Start/Tag::End. The text content is preserved but the
    // formatting is not yet applied.  When Typst adds Tagged to SmallcapsElem,
    // the handler will automatically start emitting w:smallCaps.
    assert!(
        doc_xml.contains("Small Caps"),
        "document.xml should preserve the text 'Small Caps' even without formatting"
    );
}

#[test]
fn bold_link_preserves_formatting_in_hyperlink() {
    let doc_xml = fixture_doc_xml("bold_link");

    // The hyperlink should exist
    assert!(doc_xml.contains("HYPERLINK"), "should have HYPERLINK field");
    assert!(
        doc_xml.contains("https://example.com"),
        "should have the URL"
    );
    // The bold formatting should be preserved inside the fldSimple
    assert!(
        doc_xml.contains("Bold link text"),
        "should have the display text"
    );
    assert!(
        doc_xml.contains("<w:b/>"),
        "hyperlink runs should have <w:b/> for bold formatting"
    );
}

#[test]
fn bold_link_document_model_has_bold_runs() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let doc = fixture_document("bold_link");

    // Find the hyperlink inline element and check its runs are bold
    let has_bold_link = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines.iter().any(|i| {
                if let InlineElement::Hyperlink { runs, .. } = i {
                    runs.iter()
                        .any(|r| r.bold && r.text.contains("Bold link text"))
                } else {
                    false
                }
            })
        } else {
            false
        }
    });
    assert!(
        has_bold_link,
        "document model should have a Hyperlink with bold runs containing 'Bold link text'"
    );
}

#[test]
fn edge_blockquote_has_left_indent() {
    let doc_xml = fixture_doc_xml("edge_blockquote");

    assert!(
        doc_xml.contains("w:ind w:left=\""),
        "blockquote paragraphs should have w:ind w:left set"
    );
    assert!(
        !doc_xml.contains("w:ind w:left=\"0\""),
        "blockquote indent should not be zero"
    );
}

#[test]
fn lang_german_is_de_de_not_guessed() {
    // A German document (no CJK) must derive de-DE from #set text(lang: "de"),
    // not fall back to the en-US/zh-CN guess. Guards against language-specific guesses.
    let styles = fixture_styles_xml("style_lang_de");
    assert!(
        styles.contains(r#"w:val="de-DE""#),
        "German doc should emit w:lang w:val=de-DE, got: {styles}"
    );
}

#[test]
fn lang_japanese_eastasia_is_ja_jp() {
    // Regression: a Japanese document declares #set text(lang: "ja"). It must
    // map to ja-JP on the East-Asian tag — previously mislabeled en-US because
    // only "zh" was recognized.
    let styles = fixture_styles_xml("style_cjk_ja");
    assert!(
        styles.contains(r#"w:eastAsia="ja-JP""#),
        "Japanese doc should emit w:eastAsia=ja-JP, got: {styles}"
    );
}

#[test]
fn lang_chinese_eastasia_is_zh_cn() {
    // #set text(lang: "zh", region: "CN") → zh-CN on the East-Asian tag.
    let styles = fixture_styles_xml("style_cjk_zh");
    assert!(
        styles.contains(r#"w:eastAsia="zh-CN""#),
        "Chinese doc should emit w:eastAsia=zh-CN, got: {styles}"
    );
}

#[test]
fn code_block_style_has_background_shading() {
    // The CodeBlock paragraph style carries a light-gray background (w:shd),
    // the Word convention for code blocks.
    let styles = fixture_styles_xml("style_code_blocks");
    assert!(
        styles.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="F2F2F2"/>"#),
        "CodeBlock style should include w:shd background shading, got: {styles}"
    );
}

// ── Smoke tests: style / general / edge fixtures convert without panic ──

macro_rules! smoke_test {
    ($name:ident, $fixture:expr) => {
        #[test]
        fn $name() {
            let package = fixture_package($fixture);
            assert!(
                package.byte_len() > 100,
                "docx output should be non-trivial"
            );
        }
    };
}

smoke_test!(smoke_style_default, "style_default");
smoke_test!(smoke_style_indent, "style_indent");
smoke_test!(smoke_style_justify, "style_justify");
smoke_test!(smoke_style_large_font, "style_large_font");
smoke_test!(smoke_style_small_font, "style_small_font");
smoke_test!(smoke_style_custom_font, "style_custom_font");
smoke_test!(smoke_style_custom_page, "style_custom_page");
smoke_test!(smoke_style_custom_spacing, "style_custom_spacing");
smoke_test!(smoke_style_custom_leading, "style_custom_leading");
smoke_test!(smoke_style_wide_leading, "style_wide_leading");
smoke_test!(smoke_style_no_spacing, "style_no_spacing");
smoke_test!(smoke_style_heading_custom, "style_heading_custom");
smoke_test!(smoke_style_code_blocks, "style_code_blocks");
smoke_test!(smoke_style_columns, "style_columns");
smoke_test!(smoke_style_links, "style_links");
smoke_test!(smoke_style_links_colored, "style_links_colored");
smoke_test!(smoke_style_mixed_content, "style_mixed_content");
smoke_test!(smoke_style_asymmetric_margins, "style_asymmetric_margins");
smoke_test!(smoke_style_cjk_zh, "style_cjk_zh");
smoke_test!(smoke_style_cjk_ja, "style_cjk_ja");
smoke_test!(smoke_general_elements, "general_elements");
smoke_test!(smoke_business_report, "business_report");
smoke_test!(smoke_memo, "memo");
smoke_test!(smoke_tech_doc, "tech_doc");
smoke_test!(
    smoke_edge_text_deco_across_math,
    "edge_text_deco_across_math"
);

#[test]
fn run_coalescing_collapses_split_line() {
    // Regression: the HTML walk emits one <w:r> per Typst text/space node, so a
    // plain line is shattered into many runs. The coalescing post-pass merges
    // adjacent equally-formatted runs while preserving the bold boundary.
    // See the `edge_run_coalescing` fixture.
    let doc_xml = fixture_doc_xml("edge_run_coalescing");
    let para = paragraph_containing(&doc_xml, "plain line");

    // Count runs in just this paragraph (both `<w:r>` and `<w:r ...>` forms).
    let run_count = para.matches("<w:r>").count() + para.matches("<w:r ").count();

    // Without coalescing this line is ~10+ runs; merged it is the plain head,
    // the bold word, and the plain tail — at most a handful.
    assert!(
        run_count <= 4,
        "expected the plain line to collapse to <=4 runs, got {run_count}"
    );

    // The bold word must remain its OWN run (boundary preserved): exactly one
    // run in this paragraph carries <w:b/>.
    assert_eq!(
        para.matches("<w:b/>").count(),
        1,
        "the bold span must stay a separate styled run"
    );
}

#[test]
fn hanging_indent_par_set_rule_is_honored() {
    // `#set par(hanging-indent: 2em)` is a declared value recovered from the
    // source AST: paragraphs after it get a hanging indent, the one before stays
    // flush. No genre/keyword matching. See the `edge_hanging_indent_par` fixture.
    let doc_xml = fixture_doc_xml("edge_hanging_indent_par");
    let before = paragraph_containing(&doc_xml, "before the rule");
    let after = paragraph_containing(&doc_xml, "after the rule should carry");
    assert!(
        !before.contains("w:hanging"),
        "paragraph before the set-rule must stay flush:\n{before}"
    );
    assert!(
        after.contains("w:hanging"),
        "paragraph after #set par(hanging-indent: 2em) must get a hanging indent:\n{after}"
    );
}

#[test]
fn hanging_indent_uses_the_authored_length() {
    let doc_xml = fixture_doc_xml("hanging_indent_exact_length");
    let paragraph = paragraph_containing(&doc_xml, "authored one-em");

    assert!(
        paragraph.contains(r#"w:left="200""#) && paragraph.contains(r#"w:hanging="200""#),
        "1em at an authored 10pt body size must become exactly 200 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_preserves_an_absolute_length() {
    let doc_xml = fixture_doc_xml("hanging_indent_absolute_length");
    let paragraph = paragraph_containing(&doc_xml, "authored eighteen-point");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "18pt must become exactly 360 twips regardless of body font size:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_a_source_variable() {
    let doc_xml = fixture_doc_xml("hanging_indent_variable");
    let paragraph = paragraph_containing(&doc_xml, "stored in a source variable");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "a variable holding 18pt must resolve to exactly 360 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_a_source_expression() {
    let doc_xml = fixture_doc_xml("hanging_indent_expression");
    let paragraph = paragraph_containing(&doc_xml, "calculated by a source expression");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "10pt + 8pt must resolve to exactly 360 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_length_multiplication() {
    let doc_xml = fixture_doc_xml("hanging_indent_multiplication");
    let paragraph = paragraph_containing(&doc_xml, "multiplied hanging indent");

    assert!(
        paragraph.contains(r#"w:left="400""#) && paragraph.contains(r#"w:hanging="400""#),
        "1em * 2 at an authored 10pt body size must become exactly 400 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_mixed_unit_addition() {
    let doc_xml = fixture_doc_xml("hanging_indent_mixed_units");
    let paragraph = paragraph_containing(&doc_xml, "mixed-unit hanging indent");

    assert!(
        paragraph.contains(r#"w:left="300""#) && paragraph.contains(r#"w:hanging="300""#),
        "1em + 5pt at an authored 10pt body size must become exactly 300 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_an_imported_constant() {
    let doc_xml = fixture_doc_xml("hanging_indent_imported_constant");
    let paragraph = paragraph_containing(&doc_xml, "imported hanging-indent constant");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "an imported 18pt constant must become exactly 360 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_applies_from_an_imported_document_template() {
    let doc_xml = fixture_doc_xml("hanging_indent_imported_template");
    let paragraph = paragraph_containing(&doc_xml, "imported template hanging indent");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "an imported template's 18pt hanging indent must become exactly 360 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_a_zero_argument_length_function() {
    let doc_xml = fixture_doc_xml("hanging_indent_function");
    let paragraph = paragraph_containing(&doc_xml, "function-derived hanging indent");

    assert!(
        paragraph.contains(r#"w:left="360""#) && paragraph.contains(r#"w:hanging="360""#),
        "a zero-argument function returning 18pt must become exactly 360 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_resolves_parenthesized_arithmetic() {
    let doc_xml = fixture_doc_xml("hanging_indent_parenthesized");
    let paragraph = paragraph_containing(&doc_xml, "parenthesized hanging-indent");

    assert!(
        paragraph.contains(r#"w:left="600""#) && paragraph.contains(r#"w:hanging="600""#),
        "(1em + 5pt) * 2 at 10pt must become exactly 600 twips:\n{paragraph}"
    );
}

#[test]
fn hanging_indent_stays_inside_its_content_scope() {
    let doc_xml = fixture_doc_xml("hanging_indent_local_scope");
    let inside = paragraph_containing(&doc_xml, "Inside the local");
    let outside = paragraph_containing(&doc_xml, "After the local");

    assert!(
        inside.contains(r#"w:hanging="200""#),
        "the local set rule must apply inside its content block:\n{inside}"
    );
    assert!(
        !outside.contains("w:hanging"),
        "the local set rule must not leak outside its content block:\n{outside}"
    );
}

#[test]
fn hanging_indent_in_an_inactive_branch_does_not_leak() {
    let doc_xml = fixture_doc_xml("hanging_indent_inactive_branch");
    let paragraph = paragraph_containing(&doc_xml, "Visible paragraph");

    assert!(
        !paragraph.contains("w:hanging"),
        "a set rule in an inactive branch must not affect visible content:\n{paragraph}"
    );
}

#[test]
fn complex_paper_handwritten_refs_get_hanging_indent() {
    // complex_paper writes `#set par(hanging-indent: 2em)` before its hand-typed
    // reference list (NOT a #bibliography()). Honoring that declared value gives
    // those reference paragraphs a hanging indent, while body paragraphs before
    // the rule stay flush.
    let doc_xml = fixture_doc_xml("complex_paper");
    let reference = paragraph_containing(&doc_xml, "创业活跃度");
    assert!(
        reference.contains("w:hanging"),
        "hand-written reference paragraph should get a hanging indent:\n{reference}"
    );
    let body = paragraph_containing(&doc_xml, "本文利用");
    assert!(
        !body.contains("w:hanging"),
        "body paragraph before the set-rule must stay flush:\n{body}"
    );
}

#[test]
fn first_line_indent_all_indents_paragraph_after_heading() {
    // `first-line-indent: (amount: 2em, all: true)` must indent the paragraph
    // that follows a heading (no firstLine="0" suppression), while the Normal
    // style carries the declared indent for it to inherit.
    // See the `edge_first_line_indent_all` fixture.
    let doc_xml = fixture_doc_xml("edge_first_line_indent_all");
    let styles_xml = fixture_styles_xml("edge_first_line_indent_all");
    // Isolate the Normal style block (heading styles legitimately carry firstLine=0).
    let normal_start = styles_xml
        .find(r#"w:styleId="Normal""#)
        .expect("Normal style present");
    let normal = &styles_xml[normal_start
        ..styles_xml[normal_start..]
            .find("</w:style>")
            .map_or(styles_xml.len(), |e| normal_start + e)];
    assert!(
        normal.contains("w:firstLine=") && !normal.contains(r#"w:firstLine="0""#),
        "Normal style must declare a non-zero first-line indent:\n{normal}"
    );
    let para = paragraph_containing(&doc_xml, "right after the heading");
    assert!(
        !para.contains(r#"w:firstLine="0""#),
        "paragraph after a heading must NOT be indent-suppressed under all:true:\n{para}"
    );
}

#[test]
fn em_first_line_indent_emits_char_based_chars() {
    // An em-based `first-line-indent` (2em) must emit the East-Asian
    // char-based `w:firstLineChars="200"` BEFORE the absolute `w:firstLine`
    // fallback in the Normal style. Word prefers firstLineChars when both are
    // present, so character-width indents survive a font change.
    // See the `issue_first_line_indent_chars` fixture.
    let styles_xml = fixture_styles_xml("issue_first_line_indent_chars");
    let normal_start = styles_xml
        .find(r#"w:styleId="Normal""#)
        .expect("Normal style present");
    let normal = &styles_xml[normal_start
        ..styles_xml[normal_start..]
            .find("</w:style>")
            .map_or(styles_xml.len(), |e| normal_start + e)];
    // Asserts both the value (200 = 2em × 100) AND the attribute ordering
    // (firstLineChars before firstLine).
    assert!(
        normal.contains(r#"w:firstLineChars="200" w:firstLine="#),
        "Normal style must emit char-based firstLineChars before firstLine:\n{normal}"
    );
}

#[test]
fn superscript_marker_size_does_not_leak_to_body_reference() {
    // A `#super[1]` affiliation marker renders small; its size must not be
    // generalized (by same-text matching) onto a body "[1]" reference marker.
    // The reference paragraph is entirely body-sized, so it carries no <w:sz>
    // override at all. See the `edge_super_marker_size` fixture.
    let doc_xml = fixture_doc_xml("edge_super_marker_size");
    let reference = paragraph_containing(&doc_xml, "first reference entry");
    assert!(
        !reference.contains("<w:sz "),
        "body reference marker must stay body-sized (no shrunk <w:sz>):\n{reference}"
    );
    // The real superscript marker keeps its vertAlign somewhere in the document.
    assert!(
        doc_xml.contains(r#"<w:vertAlign w:val="superscript"/>"#),
        "the affiliation #super[1] must remain a superscript"
    );
}

#[test]
fn superscript_run_uses_vertalign_alone_not_a_shrunk_size() {
    // A super/subscript run must NOT carry an explicit reduced <w:sz>: w:vertAlign
    // already shrinks the glyph and raises it by a fraction of the *effective* em, so
    // a pre-shrunk size collapses the raise and the mark sits mid-line instead of up
    // top (ECMA-376 §17.3.2.42). The run must emit vertAlign alone and inherit the
    // body size. See the `edge_super_marker_size` fixture.
    let doc_xml = fixture_doc_xml("edge_super_marker_size");
    for chunk in doc_xml.split("<w:r>").skip(1) {
        let run = chunk.split("</w:r>").next().unwrap_or("");
        if run.contains(r#"<w:vertAlign w:val="superscript"/>"#)
            || run.contains(r#"<w:vertAlign w:val="subscript"/>"#)
        {
            assert!(
                !run.contains("<w:sz "),
                "a super/subscript run must use vertAlign alone, not a shrunk <w:sz> \
                 (which collapses the baseline raise):\n{run}"
            );
        }
    }
}

#[test]
fn forced_line_break_emits_w_br_not_glued_text() {
    // A `\` line break inside a paragraph must become a real <w:br/>, not be dropped
    // (which glues "...line" and "Second..." into "lineSecond"). See
    // the `edge_line_break` fixture.
    let doc_xml = fixture_doc_xml("edge_line_break");
    assert!(
        doc_xml.contains("<w:br/>"),
        "a forced line break must emit <w:br/>:\n{doc_xml}"
    );
    assert!(
        !doc_xml.contains("lineSecond"),
        "words must not glue across the line break:\n{doc_xml}"
    );
}

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

    for word in ["first", "second", "alpha", "beta"] {
        assert_eq!(
            doc_xml.matches(&format!(">{word}<")).count(),
            1,
            "{word:?} must appear exactly once — a duplicate means recovery re-injected it"
        );
    }
}

#[test]
fn issue_cjk_linebreak_no_spurious_spaces() {
    let xml = fixture_doc_xml("issue_cjk_linebreak");
    assert!(
        xml.contains("这是一段中文文本，"),
        "first CJK text segment should be present"
    );
    assert!(
        xml.contains("用来测试换行是否会"),
        "second CJK text segment should be present"
    );
    // Verify no space-only text runs between consecutive CJK runs.
    // Extract all text content in order and check no space sits between CJK chars.
    let texts: Vec<&str> = xml
        .split("<w:t xml:space=\"preserve\">")
        .skip(1)
        .filter_map(|s| s.split("</w:t>").next())
        .collect();
    for w in texts.windows(3) {
        if w[1].trim().is_empty() {
            let prev_last = w[0].chars().last().unwrap_or(' ');
            let next_first = w[2].chars().next().unwrap_or(' ');
            let prev_cjk = ('\u{4E00}'..='\u{9FFF}').contains(&prev_last)
                || ('\u{3000}'..='\u{303F}').contains(&prev_last)
                || ('\u{FF00}'..='\u{FFEF}').contains(&prev_last);
            let next_cjk = ('\u{4E00}'..='\u{9FFF}').contains(&next_first)
                || ('\u{3000}'..='\u{303F}').contains(&next_first)
                || ('\u{FF00}'..='\u{FFEF}').contains(&next_first);
            assert!(
                !(prev_cjk && next_cjk),
                "spurious space between CJK chars: '{}' [space] '{}'",
                w[0],
                w[2]
            );
        }
    }
}

#[test]
fn issue_smart_quotes_preserved() {
    let xml = fixture_doc_xml("issue_smart_quotes");
    assert!(
        xml.contains("\u{201c}") || xml.contains("\u{201d}"),
        "smart double quotes should be preserved"
    );
    assert!(
        xml.contains("\u{2018}") || xml.contains("\u{2019}"),
        "smart single quotes should be preserved"
    );
    assert!(
        xml.contains("Hello, world!"),
        "quoted text content should be present"
    );
}

#[test]
fn edge_colored_text_has_color_runs() {
    let xml = fixture_doc_xml("edge_colored_text");
    assert!(
        xml.contains("red text"),
        "red text content should be present"
    );
    assert!(
        xml.contains("blue text"),
        "blue text content should be present"
    );
    assert!(
        xml.contains("Green text"),
        "green text content should be present"
    );
    assert!(
        xml.contains("w:val=\"FF4136\"") || xml.contains("w:val=\"ff4136\""),
        "red color value should be present"
    );
    assert!(
        xml.contains("w:val=\"0074D9\"") || xml.contains("w:val=\"0074d9\""),
        "blue color value should be present"
    );
    assert!(
        xml.contains("w:val=\"00AA00\"") || xml.contains("w:val=\"00aa00\""),
        "green hex color value should be present"
    );
}

#[test]
fn edge_inline_formatting_all_decorations() {
    let xml = fixture_doc_xml("edge_inline_formatting");
    assert!(
        xml.contains("strikethrough"),
        "strikethrough text should be present"
    );
    assert!(
        xml.contains("underlined"),
        "underlined text should be present"
    );
    assert!(
        xml.contains("Small Caps"),
        "small caps text should be present"
    );
    assert!(
        xml.contains("<w:strike/>"),
        "strikethrough should produce w:strike"
    );
    assert!(xml.contains("<w:u "), "underline should produce w:u");
    assert!(
        xml.contains("<w:smallCaps/>"),
        "smallcaps should produce w:smallCaps"
    );
    assert!(
        xml.contains("w:vertAlign"),
        "super/subscript should produce w:vertAlign"
    );
}

#[test]
fn edge_text_transforms_smallcaps_and_case() {
    let xml = fixture_doc_xml("edge_text_transforms");
    assert!(
        xml.contains("<w:smallCaps/>"),
        "smallcaps should produce w:smallCaps"
    );
    assert!(
        xml.contains("Chapter Title"),
        "smallcaps heading text should be present"
    );
    assert!(
        xml.contains("Heading1"),
        "heading with smallcaps should keep Heading1 style"
    );
}

#[test]
fn issue_smartquotes_locale_chars() {
    let xml = fixture_doc_xml("issue_smartquotes_locale");
    assert!(xml.contains("Citation"), "French text should be present");
    assert!(xml.contains("Zitat"), "German text should be present");
    assert!(
        xml.contains("English quote"),
        "English text should be present"
    );
    assert!(
        xml.contains("\u{ab}") || xml.contains("\u{bb}"),
        "French guillemets should be preserved"
    );
    assert!(
        xml.contains("\u{201e}"),
        "German low-9 quotation mark should be preserved"
    );
}

#[test]
fn issue_blockquote_attribution_text() {
    let xml = fixture_doc_xml("issue_blockquote_attribution");
    assert!(
        xml.contains("To be, or not to be"),
        "first quote should be present"
    );
    assert!(
        xml.contains("All that glitters"),
        "second quote should be present"
    );
    assert!(xml.contains("Imagination"), "third quote should be present");
    assert!(
        xml.contains("Shakespeare"),
        "first attribution should be present"
    );
    assert!(
        xml.contains("Einstein"),
        "second attribution should be present"
    );
}

#[test]
fn issue_cjk_bold_punct_formatting() {
    let xml = fixture_doc_xml("issue_cjk_bold_punct");
    assert!(xml.contains("加粗"), "bold CJK text should be present");
    assert!(xml.contains("斜体"), "italic text should be present");
    assert!(xml.contains("<w:b/>"), "bold formatting should be present");
    assert!(
        xml.contains("<w:i/>"),
        "italic formatting should be present"
    );
}

#[test]
fn issue_text_deco_inline_math_decorations() {
    let xml = fixture_doc_xml("issue_text_deco_inline_math");
    assert!(
        xml.contains("underlined"),
        "underline text should be present"
    );
    assert!(
        xml.contains("highlighted"),
        "highlight text should be present"
    );
    assert!(
        xml.contains("struck-through"),
        "strikethrough text should be present"
    );
    assert!(xml.contains("<w:u "), "underline should produce w:u");
    assert!(
        xml.contains("<w:strike"),
        "strikethrough should produce w:strike"
    );
}

#[test]
fn issue_color_primitives_text() {
    let xml = fixture_doc_xml("issue_color_primitives");
    assert!(xml.contains("RGB colored"), "RGB text should be present");
    assert!(
        xml.contains("Lightened blue"),
        "lightened color text should be present"
    );
    assert!(
        xml.contains("Named color"),
        "named color text should be present"
    );
    assert!(
        xml.matches("<w:color").count() >= 3,
        "colored text should produce w:color elements"
    );
}

#[test]
fn issue_cjk_url_encoding_links() {
    let xml = fixture_doc_xml("issue_cjk_url_encoding");
    assert!(
        xml.matches("HYPERLINK").count() >= 2,
        "hyperlinks should be present"
    );
}

#[test]
fn issue_html_whitespace_in_styled_spans_text() {
    let xml = fixture_doc_xml("issue_html_whitespace_in_styled_spans");
    assert!(
        xml.contains("<w:u ") || xml.contains("<w:color"),
        "styled spans should be present"
    );
}

#[test]
fn issue_text_fill_color() {
    let xml = fixture_doc_xml("issue_text_fill_color");
    let color_count = xml.matches("<w:color").count();
    assert!(
        color_count >= 3,
        "should have multiple color tags for different fills, got {color_count}"
    );
    assert!(
        xml.contains("This entire paragraph is red"),
        "red paragraph present"
    );
    assert!(xml.contains("blue"), "blue text reference present");
    assert!(xml.contains("Green text"), "green text present");
}

#[test]
fn issue_space_between_styled_runs() {
    let xml = fixture_doc_xml("issue_space_between_styled_runs");
    assert!(xml.contains("bold"), "bold text present");
    assert!(xml.contains("italic"), "italic text present");
    let preserve_count = xml.matches("xml:space=\"preserve\"").count();
    assert!(
        preserve_count >= 5,
        "should preserve spaces between styled runs, got {preserve_count}"
    );
}

#[test]
fn issue_text_tracking_spacing() {
    let xml = fixture_doc_xml("issue_text_tracking_spacing");
    assert!(xml.contains("Wide tracked text"), "tracked text present");
    assert!(
        xml.contains("Tight tracked text"),
        "tight tracked text present"
    );
}

#[test]
fn issue_inline_box_fill() {
    let xml = fixture_doc_xml("issue_inline_box_fill");
    assert!(xml.contains("highlighted box"), "box content present");
    assert!(xml.contains("yellow inline"), "yellow box content present");
    assert!(xml.contains("native highlight"), "native highlight present");
    assert!(
        xml.contains("w:highlight"),
        "native highlight produces w:highlight"
    );
}

#[test]
fn issue_cjk_super_sub_metrics() {
    let xml = fixture_doc_xml("issue_cjk_super_sub_metrics");
    let valign_count = xml.matches("vertAlign").count();
    assert!(
        valign_count >= 2,
        "should have vertAlign for super/sub, got {valign_count}"
    );
    assert!(
        xml.contains("\u{4E0A}\u{6807}"),
        "Chinese superscript text present"
    );
}

#[test]
fn issue_link_show_rule_ref() {
    let xml = fixture_doc_xml("issue_link_show_rule_ref");
    assert!(
        xml.contains("bookmarkStart"),
        "should have bookmark for heading label"
    );
    assert!(xml.contains("HYPERLINK"), "should have hyperlink field");
    assert!(xml.contains("Introduction"), "heading text present");
}

#[test]
fn issue_smallcaps_text() {
    let xml = fixture_doc_xml("issue_smallcaps_text");
    let sc_count = xml.matches("smallCaps").count();
    assert!(
        sc_count >= 2,
        "should have w:smallCaps for smallcaps text, got {sc_count}"
    );
    assert!(xml.contains("Small Caps"), "smallcaps text content present");
}

#[test]
fn issue_highlight_space_preserved() {
    let xml = fixture_doc_xml("issue_highlight_space");
    assert!(xml.contains("Hello"), "highlight text present");
    assert!(xml.contains("World"), "adjacent text present");
    assert!(xml.contains("bold"), "bold text present");
}
