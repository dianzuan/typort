//! Document structure: TOC, headers/footers, columns, section breaks, pagebreaks, hrules, page numbering.

use crate::common::fixture_doc_xml;
use std::io::Cursor;
use std::path::Path;

#[test]
fn toc_produces_field_code() {
    let doc_xml = fixture_doc_xml("toc_test");

    assert!(
        doc_xml.contains("fldCharType=\"begin\""),
        "TOC should produce fldChar begin"
    );
    assert!(
        doc_xml.contains("TOC"),
        "TOC should produce TOC instruction text"
    );
    assert!(
        doc_xml.contains("fldCharType=\"end\""),
        "TOC should produce fldChar end"
    );
}

#[test]
fn toc_document_model_has_toc_inline() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/toc_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let has_toc = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineElement::FieldToc { .. }))
        } else {
            false
        }
    });
    assert!(
        has_toc,
        "document model should contain a FieldToc inline element"
    );
}

#[test]
fn header_footer_produces_xml_parts() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/header_footer_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Verify the document model has header and footer content
    assert!(
        doc.header.is_some(),
        "document should detect header from header_footer_test.typ"
    );
    assert!(
        doc.footer.is_some(),
        "document should detect footer from header_footer_test.typ"
    );

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<String> = reader.file_names().map(String::from).collect();

    // Check that header/footer XML parts exist in the ZIP
    assert!(
        names.iter().any(|n| n == "word/header1.xml"),
        "should have word/header1.xml in docx, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "word/footer1.xml"),
        "should have word/footer1.xml in docx, got: {names:?}"
    );

    // Verify header content
    let header_xml = std::io::read_to_string(reader.by_name("word/header1.xml").unwrap()).unwrap();
    assert!(
        header_xml.contains("Document Title"),
        "header1.xml should contain 'Document Title'"
    );

    // Verify footer content
    let footer_xml = std::io::read_to_string(reader.by_name("word/footer1.xml").unwrap()).unwrap();
    assert!(
        footer_xml.contains("Page footer text"),
        "footer1.xml should contain 'Page footer text'"
    );
}

#[test]
fn header_footer_text_not_in_body() {
    let doc_xml = fixture_doc_xml("header_footer_test");

    // Header/footer text should NOT leak into the document body
    assert!(
        !doc_xml.contains("Document Title"),
        "header text 'Document Title' should not appear in document body"
    );
    assert!(
        !doc_xml.contains("Page footer text"),
        "footer text 'Page footer text' should not appear in document body"
    );
}

#[test]
fn header_footer_referenced_in_sect_pr() {
    let doc_xml = fixture_doc_xml("header_footer_test");

    // The sectPr should reference header and footer
    assert!(
        doc_xml.contains("w:headerReference"),
        "sectPr should contain w:headerReference"
    );
    assert!(
        doc_xml.contains("w:footerReference"),
        "sectPr should contain w:footerReference"
    );
}

#[test]
fn columns_detected_in_document_model() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/columns_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Verify the document model detected 2 columns
    assert_eq!(
        doc.page_settings.columns,
        Some(2),
        "columns_test.typ uses #set page(columns: 2), should detect 2 columns"
    );
}

#[test]
fn wide_table_is_not_misread_as_page_columns() {
    // business_report has a 4-column #table but no page-level columns. The page
    // column count comes only from the source AST, so a wide table's aligned
    // cell edges must not be mistaken for a multi-column page layout.
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/business_report.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    assert_eq!(
        doc.page_settings.columns, None,
        "a document whose only `columns:` is on a #table must stay single-column"
    );
}

#[test]
fn page_columns_func_call_form_detected() {
    // The `#page(columns: 2)[…]` function-call form (not just `#set page(...)`)
    // must be recognized from the source AST.
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/issue_column_break.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    assert_eq!(
        doc.page_settings.columns,
        Some(2),
        "#page(columns: 2)[…] should yield 2 columns"
    );
}

#[test]
fn columns_produces_w_cols_in_xml() {
    let doc_xml = fixture_doc_xml("columns_test");

    // Should have w:cols with w:num="2" in the section properties
    assert!(
        doc_xml.contains("w:cols"),
        "two-column document should produce w:cols element in sectPr"
    );
    assert!(
        doc_xml.contains("w:num=\"2\""),
        "w:cols should have w:num=\"2\" for a two-column layout"
    );
}

#[test]
fn section_break_produces_multiple_sect_pr() {
    let doc_xml = fixture_doc_xml("section_break_test");

    // Count w:sectPr elements — should be at least 2 (one inline break + final section)
    let sect_pr_count = doc_xml.matches("<w:sectPr>").count();
    assert!(
        sect_pr_count >= 2,
        "section_break_test should produce at least 2 w:sectPr elements, got {sect_pr_count}"
    );

    // Verify the section break type is nextPage
    assert!(
        doc_xml.contains("<w:type w:val=\"nextPage\"/>"),
        "section break should have type nextPage"
    );
}

#[test]
fn section_break_document_model_has_section_break() {
    use typort_ooxml::document::{BlockElement, SectionBreakType};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/section_break_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with section breaks in the document model
    let section_breaks: Vec<_> = doc
        .body
        .elements
        .iter()
        .filter_map(|e| {
            if let BlockElement::Paragraph(p) = e {
                p.section_break.as_ref()
            } else {
                None
            }
        })
        .collect();

    assert!(
        !section_breaks.is_empty(),
        "document model should have at least one section break"
    );
    assert_eq!(
        section_breaks[0].break_type,
        SectionBreakType::NextPage,
        "section break should be NextPage type"
    );
    assert!(
        section_breaks[0].page_settings.is_some(),
        "section break should carry page settings for the ending section"
    );
}

#[test]
fn section_break_has_content_from_both_sections() {
    let doc_xml = fixture_doc_xml("section_break_test");

    // Both sections' content should appear in the document
    assert!(
        doc_xml.contains("First Section"),
        "document should contain 'First Section' heading"
    );
    assert!(
        doc_xml.contains("Second Section"),
        "document should contain 'Second Section' heading"
    );
}

#[test]
fn auto_pagination_does_not_insert_hard_breaks() {
    // A document with no explicit #pagebreak() must reflow in Word, not be frozen
    // with hard page breaks at automatic page boundaries (which used to orphan tall
    // display equations onto the next page). See edge_auto_pagination_no_break.typ.
    let doc_xml = fixture_doc_xml("edge_auto_pagination_no_break");
    assert!(
        !doc_xml.contains(r#"<w:br w:type="page""#),
        "automatic page-flow boundaries must not produce hard page breaks"
    );
}

#[test]
fn pagebreak_inserts_w_br_page() {
    let doc_xml = fixture_doc_xml("pagebreak_test");

    // The document should contain a page break element
    assert!(
        doc_xml.contains("w:type=\"page\""),
        "pagebreak_test should produce a w:br with type=page"
    );

    // Both sections' content should be present
    assert!(
        doc_xml.contains("First Section"),
        "document should contain 'First Section' heading"
    );
    assert!(
        doc_xml.contains("Second Section"),
        "document should contain 'Second Section' heading"
    );
}

#[test]
fn pagebreak_document_model_has_pagebreak_inline() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/pagebreak_test.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // At least one paragraph should contain a PageBreak inline element
    let has_pagebreak = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineElement::PageBreak))
        } else {
            false
        }
    });
    assert!(
        has_pagebreak,
        "document model should contain at least one PageBreak inline element"
    );
}

/// Regression test: a `#pagebreak()` after content filling >85% of the page
/// must still produce a page break.  The old 85%-height heuristic missed this;
/// the introspector-based approach detects it correctly.
#[test]
fn pagebreak_after_nearly_full_page_is_detected() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/pagebreak_full_page.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // The document model must contain at least one PageBreak inline element.
    let has_pagebreak = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineElement::PageBreak))
        } else {
            false
        }
    });
    assert!(
        has_pagebreak,
        "pagebreak after >85%-full page must be detected"
    );

    // Verify the content from page two is present.
    let has_page2_text = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.text_runs().any(|r| r.text.contains("page two"))
        } else {
            false
        }
    });
    assert!(has_page2_text, "document should contain text from page two");
}

#[test]
fn hrule_produces_paragraph_with_bottom_border() {
    let doc_xml = fixture_doc_xml("hrule_test");

    // The document should contain a paragraph border for horizontal rules
    assert!(
        doc_xml.contains("w:pBdr"),
        "hrule_test should produce a w:pBdr element for horizontal rules"
    );
    assert!(
        doc_xml.contains("w:bottom"),
        "hrule_test should produce a w:bottom border element"
    );
}

#[test]
fn hrule_document_model_has_horizontal_rule_flag() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hrule_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // At least one paragraph should have the horizontal_rule flag set
    let has_hrule = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            p.horizontal_rule
        } else {
            false
        }
    });
    assert!(
        has_hrule,
        "document model should contain at least one paragraph with horizontal_rule=true"
    );
}

#[test]
fn hrule_content_is_preserved() {
    let doc_xml = fixture_doc_xml("hrule_test");

    // The text around the horizontal rules should be preserved
    assert!(
        doc_xml.contains("above the line"),
        "document should contain text 'above the line'"
    );
    assert!(
        doc_xml.contains("below the line"),
        "document should contain text 'below the line'"
    );
}

#[test]
fn page_numbering_typ_generates_page_field_footer() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/page_numbering.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Page numbering should be detected
    assert!(
        doc.page_numbering.is_some(),
        "should detect page numbering from #set page(numbering: \"1\")"
    );

    // Static footer should NOT be set (page number is handled by page_numbering)
    assert!(
        doc.footer.is_none(),
        "static footer should be None when page numbering is detected"
    );

    // Write to docx and verify footer XML
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<String> = reader.file_names().map(String::from).collect();

    assert!(
        names.iter().any(|n| n == "word/footer1.xml"),
        "docx should contain word/footer1.xml, got: {names:?}"
    );

    let footer_xml = std::io::read_to_string(reader.by_name("word/footer1.xml").unwrap()).unwrap();

    // Footer should contain PAGE field code
    assert!(
        footer_xml.contains(" PAGE "),
        "footer1.xml should contain PAGE instrText: {footer_xml}"
    );
    assert!(
        footer_xml.contains(r#"w:fldCharType="begin"#),
        "footer should contain fldChar begin: {footer_xml}"
    );
    assert!(
        footer_xml.contains(r#"w:fldCharType="end"#),
        "footer should contain fldChar end: {footer_xml}"
    );

    // Document body should reference the footer
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:footerReference"),
        "sectPr should reference footer: {doc_xml}"
    );
    // Should have pgNumType
    assert!(
        doc_xml.contains("w:pgNumType"),
        "sectPr should contain pgNumType: {doc_xml}"
    );
}

#[test]
fn source_line_call_still_produces_a_rule() {
    // Guard the gate: a document that DOES declare `#line()` must still get its
    // horizontal rule (the gate only suppresses invented ones).
    let doc_xml = fixture_doc_xml("hrule_test");
    assert!(
        doc_xml.contains("<w:pBdr>"),
        "a real #line() must still render as a horizontal rule:\n{doc_xml}"
    );
}

#[test]
fn pagebreak_after_a_list_lands_after_the_list() {
    // A #pagebreak() after an ordered list (before the next heading) must land after
    // the LAST list item, not before the list — the anchor must descend into the
    // nested list-item markup. See tests/fixtures/pagebreak_after_list.typ.
    let doc_xml = fixture_doc_xml("pagebreak_after_list");
    let last_item = doc_xml
        .find("Last recommendation item.")
        .expect("last item present");
    let brk = doc_xml
        .find(r#"<w:br w:type="page"/>"#)
        .expect("page break present");
    let refs = doc_xml
        .find("References")
        .expect("References heading present");
    assert!(
        last_item < brk && brk < refs,
        "page break must sit after the last list item and before References \
         (last_item={last_item}, break={brk}, refs={refs})"
    );
}

#[test]
fn page_number_footer_does_not_leak_into_body() {
    // Regression: `#set page(numbering: "i")` renders footer page numbers (ii, iii,
    // iv, …) in the bottom margin. The recovery line scraper had no body-zone
    // y-filter, so it scraped that footer text as candidate body lines; the `<2
    // chars` guard only hid single-character pages, so multi-char ones survived
    // and were emitted as centered bare-number paragraphs. The fix filters every
    // page's text items to the body zone (the same margin boundary the footer
    // detector uses to LOCATE the footer) before they become candidate lines.
    // See tests/fixtures/edge_page_number_not_in_body.typ.
    let world = typort_core::TyportWorld::new(Path::new(
        "../../tests/fixtures/edge_page_number_not_in_body.typ",
    ))
    .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Collect each body paragraph's joined run text (w:t only).
    let para_texts: Vec<String> = doc_xml
        .match_indices("<w:p>")
        .map(|(start, _)| {
            let end = doc_xml[start..]
                .find("</w:p>")
                .map_or(doc_xml.len(), |e| start + e);
            let block = &doc_xml[start..end];
            let mut t = String::new();
            let mut rest = block;
            while let Some(o) = rest.find("<w:t") {
                let after = &rest[o..];
                if let Some(gt) = after.find('>') {
                    let content = &after[gt + 1..];
                    if let Some(close) = content.find("</w:t>") {
                        t.push_str(&content[..close]);
                        rest = &content[close..];
                        continue;
                    }
                }
                break;
            }
            t
        })
        .collect();

    // A bare page-number paragraph is 1..=4 chars drawn from roman-numeral
    // letters and decimal digits only (matches `^[ivxlcdm0-9]{1,4}$`,
    // case-insensitive). No body paragraph may be one — the footer "ii"/"iii"/"iv"
    // this fixture renders must be stripped, not surface as centered text.
    let is_bare_page_number = |t: &str| {
        let t = t.trim();
        let n = t.chars().count();
        (1..=4).contains(&n)
            && t.chars().all(|c| {
                matches!(
                    c.to_ascii_lowercase(),
                    'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'
                ) || c.is_ascii_digit()
            })
    };
    let leaked: Vec<&String> = para_texts
        .iter()
        .filter(|t| is_bare_page_number(t))
        .collect();
    assert!(
        leaked.is_empty(),
        "footer page numbers must not leak into body as bare-number paragraphs, found: {leaked:?}\nall paragraphs: {para_texts:?}"
    );

    // The legitimate footer field must still be present (page numbering intact).
    let footer_xml = std::io::read_to_string(reader.by_name("word/footer1.xml").unwrap()).unwrap();
    assert!(
        footer_xml.contains(" PAGE "),
        "footer1.xml should retain the PAGE field: {footer_xml}"
    );
}
