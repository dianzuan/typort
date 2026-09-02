//! Homogeneous block of `issue_*`/`edge_*` regression fixtures, kept as one module.

use crate::common::{fixture_doc_xml, fixture_styles_xml, paragraph_containing};

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
fn issue_context_equation_no_duplicate() {
    let xml = fixture_doc_xml("issue_context_equation");
    assert!(
        xml.contains("The value is"),
        "context block text should be present"
    );
    assert!(
        xml.contains("<m:oMath>"),
        "inline equation should be present as OMML"
    );
    let p_count = xml.matches("<w:p>").count() + xml.matches("<w:p ").count();
    assert_eq!(
        p_count, 2,
        "should have exactly 2 paragraphs (context content + normal text), got {p_count}"
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
fn issue_caption_not_duplicated_by_recovery() {
    // A figure/table caption must appear exactly once: the recovery pass must
    // not re-insert it as a duplicate paragraph. Guaranteed by semantic text
    // dedup, not by hardcoded "图 "/"表 " keyword skipping.
    // CJK captions exercise the very path the old keyword filter special-cased.
    let xml = fixture_doc_xml("issue_caption_dedup_cjk");
    for caption in ["一个矩形示意图的标题", "实验数据汇总表"] {
        let count = xml.matches(caption).count();
        assert_eq!(
            count, 1,
            "caption {caption:?} should appear exactly once, found {count} (recovery duplicate?)"
        );
    }
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
fn issue_inline_math_spacing_preserved() {
    let xml = fixture_doc_xml("issue_inline_math_spacing");
    assert!(
        xml.contains("<m:oMath>"),
        "inline math should produce OMML elements"
    );
    assert!(xml.contains("Let"), "text 'Let' should be present");
    assert!(
        xml.contains("be a variable"),
        "text 'be a variable' should be present"
    );
    let p_count = xml.matches("<w:p>").count() + xml.matches("<w:p ").count();
    assert_eq!(
        p_count, 3,
        "should have exactly 3 paragraphs (one per sentence), got {p_count}"
    );
}

#[test]
fn issue_mat_delimiter_omml() {
    let xml = fixture_doc_xml("issue_mat_delimiter");
    assert!(
        xml.contains("<m:begChr m:val=\"[\""),
        "matrix with delim '[' should have begChr='['"
    );
    assert!(
        xml.contains("<m:endChr m:val=\"]\""),
        "matrix with delim '[' should have endChr=']'"
    );
    assert!(
        xml.contains("<m:m>"),
        "matrices should produce m:m elements"
    );
}

#[test]
fn issue_rotate_content_recovered() {
    let xml = fixture_doc_xml("issue_rotate_content");
    assert!(
        xml.contains("This text is normal."),
        "normal text before rotate should be present"
    );
    assert!(
        xml.contains("This text has zero rotation."),
        "rotated content should be recovered from PagedDocument"
    );
    assert!(
        xml.contains("This text follows rotated content."),
        "normal text after rotate should be present"
    );
}

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
fn issue_display_math_in_list_numbering() {
    let xml = fixture_doc_xml("issue_display_math_in_list");
    assert!(
        xml.contains("First item"),
        "first list item should be present"
    );
    assert!(
        xml.contains("Third item"),
        "third list item should be present"
    );
    let num_id_count = xml.matches("w:numId").count();
    assert!(
        num_id_count >= 3,
        "should have at least 3 list items with numId, got {num_id_count}"
    );
}

#[test]
fn issue_nested_enum_all_items_present() {
    let xml = fixture_doc_xml("issue_nested_enum_reset");
    for text in [
        "Parent A",
        "Parent B",
        "Parent C",
        "Sub one",
        "Sub two",
        "Sub one again",
    ] {
        assert!(xml.contains(text), "nested enum should contain '{text}'");
    }
}

#[test]
fn issue_subscript_scope_omml() {
    let xml = fixture_doc_xml("issue_subscript_scope");
    assert!(
        xml.contains("<m:sSub>") || xml.contains("<m:sSup>"),
        "subscript/superscript math should produce m:sSub/m:sSup"
    );
    let math_count = xml.matches("<m:oMathPara>").count();
    assert!(
        math_count >= 4,
        "should have 4 display math equations, got {math_count}"
    );
}

#[test]
fn issue_tight_list_no_duplicate() {
    let xml = fixture_doc_xml("issue_tight_list_sublist");
    let item1_count = xml.matches("Item 1").count();
    assert_eq!(
        item1_count, 1,
        "'Item 1' should appear exactly once (no recovery duplication), got {item1_count}"
    );
    assert!(xml.contains("Sub-item A"), "sub-item should be present");
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
fn edge_academic_template_structure() {
    let xml = fixture_doc_xml("edge_academic_template");
    for text in [
        "Introduction",
        "Main Results",
        "Definitions",
        "Theorem",
        "Conclusion",
        "Supplementary",
    ] {
        assert!(xml.contains(text), "heading '{text}' should be present");
    }
    assert!(
        xml.contains("Heading1"),
        "level-1 headings should use Heading1 style"
    );
    assert!(
        xml.contains("Heading2"),
        "level-2 headings should use Heading2 style"
    );
    assert!(xml.contains("Abstract"), "abstract text should be present");
    assert!(xml.contains("Keywords"), "keywords should be present");
    assert!(xml.contains("Convergence"), "title word should be present");
    assert!(
        xml.contains("<m:oMathPara>"),
        "display math should produce OMML"
    );
}

#[test]
fn edge_augmented_matrix_omml() {
    let xml = fixture_doc_xml("edge_augmented_matrix");
    assert!(
        xml.matches("<m:m>").count() >= 4,
        "should have at least 4 matrices"
    );
    assert!(
        xml.contains("<m:oMathPara>"),
        "matrices should be in display math"
    );
    assert!(
        xml.contains("cases") || xml.contains("<m:eqArr>") || xml.contains("<m:d>"),
        "cases construct should produce m:d or m:eqArr"
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
fn edge_complex_table_merges() {
    let xml = fixture_doc_xml("edge_complex_table");
    assert!(
        xml.contains("Header A-B"),
        "colspan header should be present"
    );
    assert!(
        xml.contains("Header C-D"),
        "second colspan header should be present"
    );
    assert!(
        xml.contains("Full width footer"),
        "full-width footer should be present"
    );
    assert!(
        xml.contains("w:gridSpan"),
        "colspan cells should produce w:gridSpan"
    );
    assert!(
        xml.contains("w:vMerge"),
        "rowspan cells should produce w:vMerge"
    );
}

#[test]
fn edge_custom_enum_numbering_items() {
    let xml = fixture_doc_xml("edge_custom_enum_numbering");
    for text in [
        "First major point",
        "Alpha item",
        "Top level",
        "First clause",
    ] {
        assert!(xml.contains(text), "enum item '{text}' should be present");
    }
    let num_id_count = xml.matches("w:numId").count();
    assert!(
        num_id_count >= 10,
        "should have many list items with numId, got {num_id_count}"
    );
}

#[test]
fn edge_deep_nested_list_all_levels() {
    let xml = fixture_doc_xml("edge_deep_nested_list");
    for text in [
        "Level 0",
        "Level 1",
        "Level 2",
        "Level 3",
        "Bullet parent",
        "Ordered child",
        "Bullet grandchild",
    ] {
        assert!(xml.contains(text), "list item '{text}' should be present");
    }
    for level in ["0", "1", "2", "3"] {
        assert!(
            xml.contains(&format!("w:ilvl w:val=\"{level}\"")),
            "indent level {level} should be present"
        );
    }
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
fn edge_landscape_pages_orientation() {
    let xml = fixture_doc_xml("edge_landscape_pages");
    assert!(
        xml.contains("Portrait Section"),
        "portrait heading should be present"
    );
    assert!(
        xml.contains("Landscape Section"),
        "landscape heading should be present"
    );
    assert!(
        xml.contains("Back to Portrait"),
        "return-to-portrait heading should be present"
    );
    assert!(
        xml.contains("orient"),
        "landscape section should produce orient attribute"
    );
    let sect_count = xml.matches("<w:sectPr>").count() + xml.matches("<w:sectPr ").count();
    assert!(
        sect_count >= 3,
        "should have at least 3 section breaks for orientation changes, got {sect_count}"
    );
}

#[test]
fn edge_mixed_list_content_all_present() {
    let xml = fixture_doc_xml("edge_mixed_list_content");
    for text in [
        "quadratic formula",
        "First item",
        "continuation paragraph",
        "Summary of results",
        "Outer numbered",
        "Deepest bullet",
    ] {
        assert!(xml.contains(text), "content '{text}' should be present");
    }
    assert!(
        xml.matches("w:numId").count() >= 10,
        "should have many list items with numId"
    );
}

#[test]
fn edge_multi_section_different_page_sizes() {
    let xml = fixture_doc_xml("edge_multi_section");
    for text in ["Section One", "Section Two", "Section Three"] {
        assert!(xml.contains(text), "heading '{text}' should be present");
    }
    let sect_count = xml.matches("<w:sectPr>").count() + xml.matches("<w:sectPr ").count();
    assert!(
        sect_count >= 3,
        "should have at least 3 section properties, got {sect_count}"
    );
    assert!(
        xml.contains("w:w=\"11906\""),
        "A4 width (11906 twips) should be present"
    );
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
fn edge_term_list_bold_terms() {
    let xml = fixture_doc_xml("edge_term_list");
    for term in [
        "Supervised Learning",
        "Unsupervised Learning",
        "Reinforcement Learning",
    ] {
        assert!(xml.contains(term), "term '{term}' should be present");
    }
    assert!(
        xml.contains("Training a model"),
        "definition text should be present"
    );
    assert!(
        xml.matches("<w:b/>").count() >= 3,
        "term labels should be bold"
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
fn edge_bordered_blocks_text_preserved() {
    let xml = fixture_doc_xml("edge_bordered_blocks");
    assert!(
        xml.contains("full border"),
        "bordered block text should be present"
    );
    assert!(
        xml.contains("important remark"),
        "admonition text should be present"
    );
    assert!(
        xml.contains("gray background"),
        "filled block text should be present"
    );
    assert!(
        xml.contains("Handle with care"),
        "rect text should be present"
    );
    assert!(
        xml.contains("Outer block content"),
        "nested outer text should be present"
    );
    assert!(
        xml.contains("Inner nested block"),
        "nested inner text should be present"
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
fn issue_layout_dropped_text_recovered() {
    let xml = fixture_doc_xml("issue_layout_dropped");
    assert!(
        xml.contains("Some text before"),
        "text before layout should be present"
    );
    assert!(
        xml.contains("Some text after"),
        "text after layout should be present"
    );
}

#[test]
fn issue_split_paragraph_content() {
    let xml = fixture_doc_xml("issue_split_paragraph");
    assert!(
        xml.contains("following items"),
        "intro text should be present"
    );
    assert!(xml.contains("Item one"), "list item should be present");
    assert!(
        xml.contains("Consider the equation"),
        "equation intro should be present"
    );
    assert!(
        xml.contains("<m:oMathPara>"),
        "display math should be present"
    );
    assert!(
        xml.contains("normal paragraph"),
        "trailing text should be present"
    );
}

#[test]
fn issue_table_hline_border_structure() {
    let xml = fixture_doc_xml("issue_table_hline_border");
    assert!(xml.contains("Column A"), "header cell A should be present");
    assert!(xml.contains("Column B"), "header cell B should be present");
    assert!(xml.contains("Data 1"), "data cell should be present");
    assert!(xml.contains("<w:tbl>"), "table should be present");
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
fn issue_long_crossref_label_bookmarks() {
    let xml = fixture_doc_xml("issue_long_crossref_label");
    assert!(
        xml.contains("very long heading"),
        "long heading text should be present"
    );
    assert!(
        xml.contains("Short heading"),
        "short heading text should be present"
    );
    let bookmark_count = xml.matches("w:bookmarkStart").count();
    assert!(
        bookmark_count >= 2,
        "should have at least 2 bookmarks, got {bookmark_count}"
    );
    assert!(
        !xml.contains("w:name=\"very-long-heading-label-name-exceeds-forty\""),
        "bookmark name >40 chars should be truncated"
    );
    assert!(
        xml.contains("w:name=\"very-long-heading-label-name-exceeds-for\""),
        "truncated bookmark should be exactly 40 chars"
    );
}

#[test]
fn issue_mixed_list_numbering_all_items() {
    let xml = fixture_doc_xml("issue_mixed_list_numbering");
    for text in [
        "First ordered",
        "Bullet sub-item A",
        "Bullet sub-item B",
        "Second ordered",
        "Third ordered",
    ] {
        assert!(xml.contains(text), "list item '{text}' should be present");
    }
    let num_id_count = xml.matches("w:numId").count();
    assert!(
        num_id_count >= 6,
        "should have at least 6 list items with numId, got {num_id_count}"
    );
}

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
fn issue_nested_table_structure() {
    let xml = fixture_doc_xml("issue_nested_table");
    assert!(
        xml.contains("Outer cell"),
        "outer cell text should be present"
    );
    assert!(
        xml.contains("Inner A"),
        "inner table cell should be present"
    );
    let table_count = xml.matches("<w:tbl>").count();
    assert!(
        table_count >= 2,
        "should have at least 2 tables (outer + inner), got {table_count}"
    );
}

#[test]
fn issue_table_cell_paragraph_style_content() {
    let xml = fixture_doc_xml("issue_table_cell_paragraph_style");
    assert!(
        xml.contains("normal paragraph"),
        "body paragraph should be present"
    );
    assert!(
        xml.contains("Table cell content"),
        "table cell should be present"
    );
    assert!(xml.contains("<w:tbl>"), "table should be present");
    assert!(
        xml.matches("w:numId").count() >= 2,
        "list items should have numId"
    );
}

#[test]
fn issue_complex_math_chain_accents() {
    let xml = fixture_doc_xml("issue_complex_math_chain");
    assert!(
        xml.matches("<m:acc>").count() >= 2,
        "dot accent should produce m:acc elements"
    );
    assert!(
        xml.contains("<m:sSubSup>") || xml.contains("<m:sSub>"),
        "subscripts should produce m:sSubSup or m:sSub elements"
    );
    assert!(
        xml.contains("<m:oMathPara>"),
        "display math should be present"
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
fn issue_block_content_in_table_cells() {
    let xml = fixture_doc_xml("issue_block_content_in_table");
    assert!(xml.contains("Header 1"), "table header should be present");
    assert!(
        xml.contains("Regular text"),
        "regular cell should be present"
    );
    assert!(
        xml.contains("hello"),
        "code block content should be present"
    );
    assert!(xml.contains("Item one"), "list in cell should be present");
    assert!(xml.contains("<w:tbl>"), "table should be present");
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
fn issue_list_bullet_hierarchy_levels() {
    let xml = fixture_doc_xml("issue_list_bullet_hierarchy");
    for text in [
        "Level 0",
        "Level 1",
        "Level 2",
        "Level 3",
        "Back to level 0",
    ] {
        assert!(xml.contains(text), "list item '{text}' should be present");
    }
    for level in ["0", "1", "2", "3"] {
        assert!(
            xml.contains(&format!("w:ilvl w:val=\"{level}\"")),
            "indent level {level} should be present for bullets"
        );
    }
}

#[test]
fn issue_final_section_landscape_sections() {
    let xml = fixture_doc_xml("issue_final_section_landscape");
    assert!(
        xml.contains("Portrait Section"),
        "portrait heading should be present"
    );
    assert!(
        xml.contains("Landscape Section"),
        "landscape heading should be present"
    );
    assert!(xml.contains("<w:tbl>"), "table should be present");
    let sect_count = xml.matches("<w:sectPr>").count() + xml.matches("<w:sectPr ").count();
    assert!(
        sect_count >= 2,
        "should have at least 2 section properties, got {sect_count}"
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
fn issue_footnote_math_formatting_content() {
    let xml = fixture_doc_xml("issue_footnote_math_formatting");
    let fn_count = xml.matches("w:footnoteReference").count();
    assert_eq!(fn_count, 3, "should have 3 footnotes, got {fn_count}");
}

#[test]
fn issue_math_trailing_punct_equations() {
    let xml = fixture_doc_xml("issue_math_trailing_punct");
    assert!(
        xml.contains("obtain"),
        "text before equation should be present"
    );
    let math_count = xml.matches("<m:oMathPara>").count();
    assert!(
        math_count >= 2,
        "should have at least 2 display math blocks, got {math_count}"
    );
}

#[test]
fn issue_symbol_subscript_omml() {
    let xml = fixture_doc_xml("issue_symbol_subscript");
    assert!(
        xml.contains("<m:sSub>"),
        "subscript on symbol should produce m:sSub"
    );
    assert!(
        xml.contains("<m:sSup>"),
        "superscript on symbol should produce m:sSup"
    );
    let math_count = xml.matches("<m:oMathPara>").count();
    assert!(
        math_count >= 3,
        "should have at least 3 display math blocks, got {math_count}"
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
fn issue_let_math_vars_content() {
    let xml = fixture_doc_xml("issue_let_math_vars");
    assert!(
        xml.contains("<m:oMath>"),
        "interpolated math should produce OMML"
    );
}

#[test]
fn issue_show_rule_link_list_content() {
    let xml = fixture_doc_xml("issue_show_rule_link_list");
    assert!(xml.contains("Example"), "link text should be present");
    assert!(
        xml.contains("Regular item"),
        "plain list item should be present"
    );
    assert!(
        xml.contains("Normal paragraph"),
        "body text should be present"
    );
    assert!(
        xml.contains("HYPERLINK"),
        "hyperlink field should be present"
    );
    assert!(
        xml.matches("w:numId").count() >= 3,
        "list items should have numId"
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
    // See tests/fixtures/edge_heading_smartquotes.typ.
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
fn issue_math_grouping_attach_omml() {
    let xml = fixture_doc_xml("issue_math_grouping_attach");
    assert!(
        xml.contains("<m:sSubSup>"),
        "combined sub+sup should produce m:sSubSup"
    );
    let math_count = xml.matches("<m:oMathPara>").count();
    assert!(
        math_count >= 4,
        "should have at least 4 display math blocks, got {math_count}"
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
fn issue_math_dot_punctuation_equations() {
    let xml = fixture_doc_xml("issue_math_dot_punctuation");
    let math_count = xml.matches("<m:oMathPara>").count();
    assert!(
        math_count >= 4,
        "should have at least 4 display math blocks, got {math_count}"
    );
}

#[test]
fn issue_section_equation_numbering_refs() {
    let xml = fixture_doc_xml("issue_section_equation_numbering");
    assert!(xml.contains("Introduction"), "heading should be present");
    assert!(xml.contains("Methods"), "second heading should be present");
    assert!(
        xml.matches("<m:oMathPara>").count() >= 3,
        "should have at least 3 display equations"
    );
    assert!(
        xml.matches("w:bookmarkStart").count() >= 3,
        "labeled equations should produce bookmarks"
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
fn issue_nested_term_list_hierarchy() {
    let xml = fixture_doc_xml("issue_nested_term_list");
    for text in [
        "Compiler",
        "Frontend",
        "Lexer",
        "Parser",
        "Backend",
        "Interpreter",
    ] {
        assert!(xml.contains(text), "term '{text}' should be present");
    }
    assert!(
        xml.matches("<w:b/>").count() >= 4,
        "term labels should be bold"
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

#[test]
fn issue_table_cell_spacing_structure() {
    let xml = fixture_doc_xml("issue_table_cell_spacing");
    assert!(xml.contains("Fruit"), "header cell should be present");
    assert!(xml.contains("Bananas"), "data cell should be present");
    assert!(
        xml.contains("Built-in wrapper"),
        "multi-paragraph cell should be present"
    );
    assert!(xml.contains("<w:tbl>"), "table should be present");
}

#[test]
fn issue_nested_list_indent_levels() {
    let xml = fixture_doc_xml("issue_nested_list_indent");
    for text in ["Level one", "Level two", "Level three", "Bullet level"] {
        assert!(xml.contains(text), "list item '{text}' should be present");
    }
    for level in ["0", "1", "2", "3"] {
        assert!(
            xml.contains(&format!("w:ilvl w:val=\"{level}\"")),
            "indent level {level} should be present"
        );
    }
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
fn issue_list_paragraph_style_items() {
    let xml = fixture_doc_xml("issue_list_paragraph_style");
    assert!(
        xml.matches("w:numId").count() >= 6,
        "should have list items with numId"
    );
}

#[test]
fn issue_table_compact_style_override_content() {
    let xml = fixture_doc_xml("issue_table_compact_style_override");
    assert!(xml.contains("<w:tbl>"), "table should be present");
    assert!(
        xml.contains("<w:b/>"),
        "bold text in table should be preserved"
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
fn issue_cjk_url_encoding_links() {
    let xml = fixture_doc_xml("issue_cjk_url_encoding");
    assert!(
        xml.matches("HYPERLINK").count() >= 2,
        "hyperlinks should be present"
    );
}

#[test]
fn issue_table_header_border_override_tables() {
    let xml = fixture_doc_xml("issue_table_header_border_override");
    let table_count = xml.matches("<w:tbl>").count();
    assert!(
        table_count >= 3,
        "should have at least 3 tables, got {table_count}"
    );
}

#[test]
fn issue_crossref_field_code_bookmarks() {
    let xml = fixture_doc_xml("issue_crossref_field_code");
    assert!(
        xml.matches("w:bookmarkStart").count() >= 3,
        "labeled elements should produce bookmarks"
    );
    assert!(
        xml.contains("<m:oMath>"),
        "display equations should be present"
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
fn issue_bookmark_inside_paragraph() {
    let xml = fixture_doc_xml("issue_bookmark_inside_paragraph");
    assert!(
        xml.contains("w:bookmarkStart"),
        "should have bookmarkStart for <intro> label"
    );
    assert!(xml.contains("w:bookmarkEnd"), "should have bookmarkEnd");
    assert!(xml.contains("Introduction"), "heading text present");
    assert!(xml.contains("Methods"), "second heading present");
    assert!(
        xml.contains("intro"),
        "bookmark name should reference intro label"
    );
}

#[test]
fn issue_rtl_table_bidi() {
    let xml = fixture_doc_xml("issue_rtl_table_bidi");
    assert!(xml.contains("w:tbl"), "should contain a table");
    assert!(
        xml.contains("\u{627}\u{644}\u{639}\u{645}\u{648}\u{62f}"),
        "Arabic text should be present"
    );
}

#[test]
fn issue_nested_table_alignment() {
    let xml = fixture_doc_xml("issue_nested_table_alignment");
    assert!(xml.contains("w:tbl"), "should contain at least one table");
    assert!(xml.contains("Normal right cell"), "outer cell text present");
}

#[test]
fn issue_list_in_blockquote() {
    let xml = fixture_doc_xml("issue_list_in_blockquote");
    assert!(xml.contains("First"), "ordered list item present");
    assert!(xml.contains("Second"), "ordered list item present");
    assert!(xml.contains("Bullet"), "bullet list item present");
    let num_count = xml.matches("w:numId").count();
    assert!(
        num_count >= 2,
        "should have multiple list numbering references, got {num_count}"
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
fn issue_list_contextual_spacing() {
    let xml = fixture_doc_xml("issue_list_contextual_spacing");
    assert!(xml.contains("Item A"), "first list item present");
    assert!(xml.contains("Item B"), "second list item present");
    let num_count = xml.matches("w:numId").count();
    assert!(
        num_count >= 5,
        "should have multiple list numbering refs, got {num_count}"
    );
}

#[test]
fn issue_table_colspan_borders() {
    let xml = fixture_doc_xml("issue_table_colspan_borders");
    assert!(xml.contains("AB"), "merged cell AB present");
    assert!(xml.contains("FGH"), "merged cell FGH present");
    let gridspan = xml.matches("gridSpan").count();
    assert!(
        gridspan >= 2,
        "should have gridSpan for merged cells, got {gridspan}"
    );
    let tc_count = xml.matches("<w:tc>").count();
    assert!(
        tc_count >= 9,
        "should have at least 9 table cells, got {tc_count}"
    );
}

#[test]
fn issue_math_accent_subsup_chain() {
    let xml = fixture_doc_xml("issue_math_accent_subsup_chain");
    let acc_count = xml.matches("m:acc").count();
    assert!(
        acc_count >= 4,
        "should have accent elements for dot/hat/tilde/arrow, got {acc_count}"
    );
    let math_para = xml.matches("oMathPara").count();
    assert!(
        math_para >= 4,
        "should have display math paragraphs, got {math_para}"
    );
    assert!(
        xml.contains("m:sSubSup") || xml.contains("m:sSub"),
        "should have sub/superscript elements"
    );
}

#[test]
fn issue_table_caption_crossref() {
    let xml = fixture_doc_xml("issue_table_caption_crossref");
    assert!(xml.contains("Sample data"), "first table caption present");
    assert!(
        xml.contains("Another table"),
        "second table caption present"
    );
    let bk_count = xml.matches("bookmarkStart").count();
    assert!(
        bk_count >= 2,
        "should have bookmarks for labeled figures, got {bk_count}"
    );
}

#[test]
fn issue_table_multipage_borders() {
    let xml = fixture_doc_xml("issue_table_multipage_borders");
    assert!(xml.contains("<w:tbl>"), "should contain a table");
    let tr_count = xml.matches("<w:tr>").count();
    assert!(
        tr_count >= 7,
        "should have at least 7 table rows, got {tr_count}"
    );
    assert!(xml.contains("Header A"), "header row present");
    assert!(xml.contains("Row 6"), "last data row present");
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
fn issue_table_cell_valign() {
    let xml = fixture_doc_xml("issue_table_cell_valign");
    assert!(xml.contains("Middle"), "middle-aligned cell text present");
    assert!(xml.contains("Bottom"), "bottom-aligned cell text present");
    assert!(xml.contains("<w:tbl>"), "should contain a table");
}

#[test]
fn issue_table_cell_shading() {
    let xml = fixture_doc_xml("issue_table_cell_shading");
    assert!(xml.contains("Yellow cell"), "yellow cell text present");
    assert!(xml.contains("Green cell"), "green cell text present");
    assert!(xml.contains("<w:tbl>"), "should contain a table");
    let tc_count = xml.matches("<w:tc>").count();
    assert!(
        tc_count >= 6,
        "should have at least 6 table cells, got {tc_count}"
    );
}

#[test]
fn issue_table_dashed_borders() {
    let xml = fixture_doc_xml("issue_table_dashed_borders");
    let tbl_count = xml.matches("<w:tbl>").count();
    assert!(tbl_count >= 2, "should have two tables, got {tbl_count}");
    assert!(xml.contains("A"), "first table content present");
    assert!(xml.contains("H"), "second table content present");
}

#[test]
fn issue_place_absolute() {
    let xml = fixture_doc_xml("issue_place_absolute");
    assert!(xml.contains("body text"), "body text present");
    assert!(xml.contains("Final paragraph"), "final paragraph present");
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
fn issue_footnote_tab_format() {
    let xml = fixture_doc_xml("issue_footnote_tab_format");
    assert!(
        xml.contains("footnote"),
        "footnote reference should be in document"
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
fn issue_column_break() {
    let xml = fixture_doc_xml("issue_column_break");
    assert!(xml.contains("First column"), "first column content present");
    assert!(
        xml.contains("Second column"),
        "second column content present"
    );
    // #colbreak() must become a real column break, not be dropped.
    assert!(
        xml.contains(r#"<w:br w:type="column"/>"#),
        "colbreak should emit <w:br w:type=\"column\"/>, got: {xml}"
    );
    // It sits after the first column's content and before the second's.
    let br = xml.find(r#"<w:br w:type="column"/>"#).unwrap();
    let first = xml.find("More first column text").unwrap();
    let second = xml.find("Second column content").unwrap();
    assert!(
        first < br && br < second,
        "column break should fall between the first and second column content"
    );
}

#[test]
fn issue_metadata_case_dedup() {
    let xml = fixture_doc_xml("issue_metadata_case_dedup");
    assert!(xml.contains("test document"), "document body text present");
}

#[test]
fn issue_highlight_space_preserved() {
    let xml = fixture_doc_xml("issue_highlight_space");
    assert!(xml.contains("Hello"), "highlight text present");
    assert!(xml.contains("World"), "adjacent text present");
    assert!(xml.contains("bold"), "bold text present");
}

#[test]
fn issue_show_rule_heading_replace_recovery() {
    let xml = fixture_doc_xml("issue_show_rule_heading_replace");
    assert!(xml.contains("First Heading"), "first heading text present");
    assert!(
        xml.contains("Second Heading"),
        "second heading text present"
    );
    assert!(xml.contains("Body text"), "body text present");
}

/// GitHub issue #4: an inline `#context [...]` inside a paragraph used to emit
/// the paragraph twice — once correct, once with the contextual content
/// stripped (`X ctx tail.` followed by a stray `X tail.`).
#[test]
fn issue_inline_context_no_duplicate_paragraph() {
    let xml = fixture_doc_xml("issue_inline_context");
    let texts: Vec<String> = crate::common::paragraph_texts(&xml)
        .into_iter()
        .filter(|t| !t.trim().is_empty())
        .collect();
    assert_eq!(
        texts,
        [
            "X ctx tail.",
            "Q 1 first.",
            "Q 2 second.",
            "block-level ctx",
        ],
        "each inline-context paragraph must appear exactly once, with its context content"
    );
}
