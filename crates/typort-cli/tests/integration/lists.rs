//! List structure, numbering, and formatting tests.

use crate::common::{
    fixture_doc_xml, fixture_document, fixture_package, fixture_part, paragraph_containing,
};

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
fn issue_list_paragraph_style_items() {
    let xml = fixture_doc_xml("issue_list_paragraph_style");
    assert!(
        xml.matches("w:numId").count() >= 6,
        "should have list items with numId"
    );
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
fn nested_list_has_multiple_levels() {
    let doc_xml = fixture_doc_xml("nested_list");

    assert!(
        doc_xml.contains(r#"w:ilvl w:val="0""#),
        "should have level 0 list items"
    );
    assert!(
        doc_xml.contains(r#"w:ilvl w:val="1""#),
        "should have level 1 (nested) list items"
    );
    assert!(
        doc_xml.contains(r#"w:ilvl w:val="2""#),
        "should have level 2 (doubly nested) list items"
    );
}

#[test]
fn nested_list_document_model_has_levels() {
    use typort_ooxml::document::BlockElement;

    let doc = fixture_document("nested_list");

    let levels: Vec<u32> = doc
        .body
        .elements
        .iter()
        .filter_map(|e| {
            if let BlockElement::Paragraph(p) = e {
                p.list_info.as_ref().map(|li| li.level)
            } else {
                None
            }
        })
        .collect();

    assert!(levels.contains(&0), "should have list items at level 0");
    assert!(levels.contains(&1), "should have list items at level 1");
    assert!(levels.contains(&2), "should have list items at level 2");
}

#[test]
fn edge_list_restart_separate_lists_get_unique_num_ids() {
    let package = fixture_package("edge_list_restart");
    let doc_xml = package.part_text("word/document.xml");
    let num_xml = package.part_text("word/numbering.xml");

    let num_ids: Vec<&str> = doc_xml
        .match_indices("w:numId w:val=\"")
        .map(|(pos, _)| {
            let start = pos + 15;
            let end = doc_xml[start..].find('"').unwrap() + start;
            &doc_xml[start..end]
        })
        .collect();
    let unique: std::collections::HashSet<&&str> = num_ids.iter().collect();
    assert!(
        unique.len() >= 3,
        "3 separate lists should have at least 3 unique numIds, got {:?}",
        num_ids
    );
    for id in &unique {
        let pattern = format!("w:numId=\"{}\"", id);
        assert!(
            num_xml.contains(&pattern),
            "numbering.xml should define numId {id}"
        );
    }
}

#[test]
fn separate_ordered_lists_each_restart_at_one() {
    // Two distinct ordered lists must each restart at 1 — the second must not
    // continue (1,2,3 then 4,5,6). They share one abstract numbering format, so
    // every <w:num> instance needs a level-0 startOverride or Word continues the
    // shared counter across lists. See the `two_ordered_lists` fixture.
    let numbering = fixture_part("two_ordered_lists", "word/numbering.xml");
    let overrides = numbering.matches(r#"<w:startOverride w:val="1"/>"#).count();
    assert!(
        overrides >= 2,
        "each ordered list's <w:num> must carry a level-0 startOverride so it restarts \
         at 1 (two lists -> >= 2 overrides); found {overrides}:\n{numbering}"
    );
}

#[test]
fn enum_custom_start_keeps_numbering() {
    let numbering = fixture_part("enum_custom_start", "word/numbering.xml");
    assert!(
        numbering.contains("<w:startOverride w:val=\"4\"/>"),
        "#enum(start: 4) must carry startOverride 4, got:\n{numbering}"
    );
    assert!(
        numbering.contains("<w:startOverride w:val=\"1\"/>"),
        "a plain enum must still restart at 1"
    );
}

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
    assert!(
        !doc_xml.contains("\u{1D465}"),
        "MathML glyphs must not leak as literal text"
    );
}

#[test]
fn hanging_indent_does_not_clobber_list_items() {
    // A `#set par(hanging-indent: 2em)` rule must not override a list item's own
    // indent. List items keep the list hanging indent (left 2em / hanging 1em =
    // 440/220 at the 11pt default), never the bibliography 2em/2em (440/440).
    // See the `edge_hanging_indent_list` fixture.
    let doc_xml = fixture_doc_xml("edge_hanging_indent_list");
    let item = paragraph_containing(&doc_xml, "list item with");
    assert!(
        item.contains(r#"w:hanging="220""#),
        "list item must keep its list hanging indent (220):\n{item}"
    );
    assert!(
        !item.contains(r#"w:hanging="440""#),
        "the hanging-indent rule must not clobber the list indent with 440:\n{item}"
    );
}
