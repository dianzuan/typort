use std::io::Cursor;
use std::path::Path;

#[test]
fn complex_paper_has_table_structure() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify w:tbl structure is present
    assert!(
        doc_xml.contains("w:tbl"),
        "document.xml should contain w:tbl element for the table"
    );
    assert!(
        doc_xml.contains("w:tr"),
        "document.xml should contain w:tr elements for table rows"
    );
    assert!(
        doc_xml.contains("w:tc"),
        "document.xml should contain w:tc elements for table cells"
    );
    assert!(
        doc_xml.contains("w:tblBorders"),
        "table should have borders defined"
    );
    // Check that table content is in cells
    assert!(
        doc_xml.contains("变量类型") || doc_xml.contains("变量名称"),
        "table cells should contain the paper's table content"
    );
}

#[test]
fn complex_paper_has_list_numbering() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify w:numPr is present for list items
    assert!(
        doc_xml.contains("w:numPr"),
        "document.xml should contain w:numPr for list items"
    );
    assert!(
        doc_xml.contains("w:ilvl"),
        "document.xml should contain w:ilvl for list level"
    );
    assert!(
        doc_xml.contains("w:numId"),
        "document.xml should contain w:numId for numbering instance"
    );

    // Verify numbering.xml exists
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n == "word/numbering.xml"),
        "docx should contain word/numbering.xml, got: {names:?}"
    );

    // Verify numbering.xml content
    let num_xml = std::io::read_to_string(reader.by_name("word/numbering.xml").unwrap()).unwrap();
    assert!(
        num_xml.contains("w:numbering"),
        "numbering.xml should have w:numbering root"
    );
    assert!(
        num_xml.contains("w:abstractNum"),
        "numbering.xml should contain abstract numbering definitions"
    );
    assert!(
        num_xml.contains("w:numFmt"),
        "numbering.xml should contain number format definitions"
    );

    // Verify content types include numbering
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("numbering"),
        "content types should reference numbering"
    );

    // Verify document rels include numbering relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("numbering"),
        "document rels should reference numbering"
    );
}

#[test]
fn end_to_end_hello_typ_to_docx() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<&str> = reader.file_names().collect();

    assert!(names.contains(&"[Content_Types].xml"));
    assert!(names.contains(&"word/document.xml"));
    assert!(names.contains(&"word/styles.xml"));
    assert!(names.contains(&"word/fontTable.xml"));

    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:document"));
    assert!(doc_xml.contains("Hello"));
    assert!(doc_xml.contains("Heading1"), "should have heading style");
    assert!(
        doc_xml.contains("w:sectPr"),
        "should have section properties"
    );
}

#[test]
fn complex_paper_has_semantic_structure() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("Heading1"), "should have Heading1");
    assert!(doc_xml.contains("Heading2"), "should have Heading2");
    assert!(doc_xml.contains("<w:b/>"), "should have bold formatting");
    assert!(doc_xml.contains("w:pgMar"), "should have page margins");
    assert!(
        doc_xml.contains("数字经济"),
        "should contain Chinese paper text"
    );
}

#[test]
fn complex_paper_has_footnotes() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Verify footnotes were detected in the document model
    assert!(!doc.footnotes.is_empty(), "should have detected footnotes");
    assert_eq!(doc.footnotes.len(), 3, "complex paper has 3 footnotes");

    // Verify footnote content was extracted
    let first_fn_text: String = doc.footnotes[0]
        .content
        .iter()
        .filter_map(|i| {
            if let typort_ooxml::document::InlineElement::Text(r) = i {
                Some(r.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        first_fn_text.contains("习近平"),
        "first footnote should contain reference text, got: {first_fn_text}"
    );

    // Write to docx and verify XML structure
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Verify footnotes.xml exists in the archive
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n == "word/footnotes.xml"),
        "docx should contain word/footnotes.xml, got: {names:?}"
    );

    // Verify document.xml has footnote references
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:footnoteReference"),
        "document.xml should contain w:footnoteReference"
    );
    assert!(
        doc_xml.contains("FootnoteReference"),
        "document.xml should reference FootnoteReference style"
    );

    // Verify footnotes.xml content
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();
    assert!(
        fn_xml.contains("w:footnotes"),
        "footnotes.xml should have w:footnotes root"
    );
    assert!(
        fn_xml.contains("w:footnoteRef"),
        "footnotes.xml should contain w:footnoteRef"
    );
    assert!(
        fn_xml.contains("习近平"),
        "footnotes.xml should contain first footnote text"
    );
    assert!(
        fn_xml.contains("Schumpeter"),
        "footnotes.xml should contain second footnote text"
    );

    // Verify content types include footnotes
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("footnotes"),
        "content types should reference footnotes"
    );

    // Verify document rels include footnotes relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
    assert!(
        rels_xml.contains("footnotes"),
        "document rels should reference footnotes"
    );
}

#[test]
fn italic_text_produces_w_i_element() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/italic_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
fn math_test_produces_omml() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify OMML namespace is present
    assert!(
        doc_xml.contains("xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\""),
        "document.xml should have the math namespace"
    );

    // Verify inline equation produces m:oMath
    assert!(
        doc_xml.contains("<m:oMath>"),
        "document.xml should contain <m:oMath> for equations"
    );

    // Verify block equation produces m:oMathPara
    assert!(
        doc_xml.contains("<m:oMathPara>"),
        "document.xml should contain <m:oMathPara> for block equations"
    );

    // Verify superscript structure (x^2)
    assert!(
        doc_xml.contains("<m:sSup>"),
        "document.xml should contain <m:sSup> for superscripts"
    );

    // Verify fraction structure (frac(n(n+1), 2))
    assert!(
        doc_xml.contains("<m:f>"),
        "document.xml should contain <m:f> for fractions"
    );
    assert!(
        doc_xml.contains("<m:num>"),
        "document.xml should contain <m:num> for fraction numerator"
    );
    assert!(
        doc_xml.contains("<m:den>"),
        "document.xml should contain <m:den> for fraction denominator"
    );

    // Verify nary (summation) structure
    assert!(
        doc_xml.contains("<m:nary>"),
        "document.xml should contain <m:nary> for summation"
    );

    // Verify delimiter structure (parentheses in n(n+1))
    assert!(
        doc_xml.contains("<m:d>"),
        "document.xml should contain <m:d> for delimiters"
    );

    // Verify math runs contain expected symbols
    assert!(
        doc_xml.contains("<m:t>x</m:t>"),
        "document.xml should contain math text 'x'"
    );
    assert!(
        doc_xml.contains("<m:t>2</m:t>"),
        "document.xml should contain math text '2'"
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

    // Load the built-in preset
    let preset = typort_presets::load_preset(Path::new("../../presets"), "管理世界").unwrap();

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
fn numbered_equation_has_right_aligned_number() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/numbered_eq.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify equation number "(1)" appears in the document
    assert!(
        doc_xml.contains("(1)"),
        "document.xml should contain equation number (1)"
    );
    // Verify equation number "(2)" appears for the second equation
    assert!(
        doc_xml.contains("(2)"),
        "document.xml should contain equation number (2)"
    );
    // Verify right-aligned tab stop is present
    assert!(
        doc_xml.contains("w:tab") && doc_xml.contains("right"),
        "document.xml should have a right-aligned tab stop for equation numbering"
    );
    // Verify the OMML equation is still present
    assert!(
        doc_xml.contains("<m:oMathPara>"),
        "document.xml should still contain the block equation"
    );
}

#[test]
fn numbered_equation_document_model_has_numbers() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/numbered_eq.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with numbered equations
    let numbered_eqs: Vec<&str> = doc
        .body
        .elements
        .iter()
        .filter_map(|e| {
            if let BlockElement::Paragraph(p) = e {
                for inline in &p.inlines {
                    if let InlineElement::Math {
                        equation_number: Some(num),
                        ..
                    } = inline
                    {
                        return Some(num.as_str());
                    }
                }
            }
            None
        })
        .collect();

    assert_eq!(
        numbered_eqs.len(),
        2,
        "should have 2 numbered equations, got {numbered_eqs:?}"
    );
    assert_eq!(numbered_eqs[0], "(1)");
    assert_eq!(numbered_eqs[1], "(2)");
}

#[test]
fn table_cell_supports_merged_cell_fields() {
    use typort_ooxml::document::{Paragraph, TableCell, VMerge};

    // Verify that the TableCell struct has the colspan/vmerge fields
    let cell = TableCell {
        paragraphs: vec![Paragraph::new()],
        content: Vec::new(),
        colspan: 2,
        vmerge: VMerge::Restart,
        width_pct: None,
    };
    assert_eq!(cell.colspan, 2);
    assert_eq!(cell.vmerge, VMerge::Restart);

    // Verify VMerge::Continue
    let cont_cell = TableCell {
        paragraphs: vec![Paragraph::new()],
        content: Vec::new(),
        colspan: 1,
        vmerge: VMerge::Continue,
        width_pct: None,
    };
    assert_eq!(cont_cell.vmerge, VMerge::Continue);
}

#[test]
fn merged_cell_emits_grid_span_and_vmerge() {
    use typort_ooxml::document::{Document, Paragraph, Table, TableCell, TableRow, VMerge};

    let mut doc = Document::new();
    let table = Table {
        rows: vec![
            TableRow {
                cells: vec![
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 2,
                        vmerge: VMerge::Restart,
                        width_pct: None,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 1,
                        vmerge: VMerge::None,
                        width_pct: None,
                    },
                ],
            },
            TableRow {
                cells: vec![
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 2,
                        vmerge: VMerge::Continue,
                        width_pct: None,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 1,
                        vmerge: VMerge::None,
                        width_pct: None,
                    },
                ],
            },
        ],
        width_pct: None,
        border_size: None,
    };
    doc.add_table(table);

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Verify gridSpan is emitted
    assert!(
        doc_xml.contains("w:gridSpan") && doc_xml.contains("w:val=\"2\""),
        "document.xml should contain w:gridSpan with val=2 for colspan=2 cell"
    );
    // Verify vMerge restart
    assert!(
        doc_xml.contains("<w:vMerge w:val=\"restart\"/>"),
        "document.xml should contain w:vMerge val=restart for rowspan start"
    );
    // Verify vMerge continue (empty element)
    assert!(
        doc_xml.contains("<w:vMerge/>"),
        "document.xml should contain w:vMerge (no val) for continuation cell"
    );
}

// ---------------------------------------------------------------------------
// Math unit integration tests – compile math_unit.typ and assert OMML output
// ---------------------------------------------------------------------------

fn math_unit_doc_xml() -> String {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/math_unit.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
}

#[test]
fn math_fraction_produces_m_f() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:f>"),
        "document.xml should contain <m:f> for fraction"
    );
    assert!(
        doc_xml.contains("<m:num>"),
        "document.xml should contain <m:num> for fraction numerator"
    );
    assert!(
        doc_xml.contains("<m:den>"),
        "document.xml should contain <m:den> for fraction denominator"
    );
}

#[test]
fn math_square_root_produces_m_rad() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:rad>"),
        "document.xml should contain <m:rad> for square root"
    );
    assert!(
        doc_xml.contains("<m:degHide m:val=\"1\"/>"),
        "square root should hide degree with <m:degHide m:val=\"1\"/>"
    );
}

#[test]
fn math_cube_root_has_degree() {
    let doc_xml = math_unit_doc_xml();
    // There should be a <m:rad> that contains <m:deg> with content (the index "3")
    assert!(
        doc_xml.contains("<m:deg>"),
        "cube root should have <m:deg> element for the index"
    );
    // The cube root's degree should contain the text "3"
    assert!(
        doc_xml.contains("<m:t>3</m:t>"),
        "cube root degree should contain the text '3'"
    );
}

#[test]
fn math_subscript_produces_m_ssub() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:sSub>"),
        "document.xml should contain <m:sSub> for subscript"
    );
}

#[test]
fn math_superscript_produces_m_ssup() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:sSup>"),
        "document.xml should contain <m:sSup> for superscript"
    );
}

#[test]
fn math_sub_and_sup_produces_m_ssubsup() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:sSubSup>"),
        "document.xml should contain <m:sSubSup> for combined sub+superscript"
    );
}

#[test]
fn math_summation_produces_m_nary() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:nary>"),
        "document.xml should contain <m:nary> for summation"
    );
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{2211}\"/>"),
        "summation should have <m:chr m:val=\"\\u{{2211}}\"/> (summation symbol)"
    );
}

#[test]
fn math_product_produces_m_nary() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{220F}\"/>"),
        "product should have <m:chr m:val=\"\\u{{220F}}\"/> (product symbol)"
    );
}

#[test]
fn math_nested_fraction() {
    let doc_xml = math_unit_doc_xml();
    // Count occurrences of <m:f> — should be at least 3: the simple frac, and 2 from nested
    let count = doc_xml.matches("<m:f>").count();
    assert!(
        count >= 3,
        "should have at least 3 <m:f> elements (1 simple + 2 nested), got {count}"
    );
}

#[test]
fn math_greek_letters() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:t>\u{03B1}</m:t>"),
        "should contain Greek alpha (\u{03B1})"
    );
    assert!(
        doc_xml.contains("<m:t>\u{03B2}</m:t>"),
        "should contain Greek beta (\u{03B2})"
    );
    assert!(
        doc_xml.contains("<m:t>\u{03B3}</m:t>"),
        "should contain Greek gamma (\u{03B3})"
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
fn features_suppress_indent_and_bibliography() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Feature 3: First paragraph after heading has firstLine="0" (suppress indent)
    // The Normal style has firstLine="420", so paragraphs after headings should override
    assert!(
        doc_xml.contains("w:firstLine=\"0\""),
        "document.xml should have firstLine=\"0\" for paragraphs after headings"
    );

    // Feature 4: Bibliography paragraphs have hanging indent
    assert!(
        doc_xml.contains("w:hanging=\"420\"") && doc_xml.contains("w:left=\"420\""),
        "document.xml should have hanging indent for bibliography entries"
    );
}

#[test]
fn features_table_width_percentage() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Content recovery tests
// ---------------------------------------------------------------------------

#[test]
fn center_test_recovers_aligned_content() {
    use typort_ooxml::document::Alignment;

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/center_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // The centered text "张三  李四" should be recovered from PagedDocument
    let has_centered_authors = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            let text = p.text_content();
            (text.contains("张三") || text.contains("李四"))
                && p.alignment == Some(Alignment::Center)
        } else {
            false
        }
    });
    assert!(
        has_centered_authors,
        "center_test should recover centered author names"
    );
}

#[test]
fn complex_paper_recovers_author_info() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // The author names and institution info from #align(center) should be recovered
    let has_author = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            p.text_runs()
                .any(|r| r.text.contains("张三") || r.text.contains("李四"))
        } else {
            false
        }
    });
    assert!(
        has_author,
        "complex paper should recover author names from #align(center)"
    );

    let has_institution = doc.body.elements.iter().any(|e| {
        if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
            p.text_runs()
                .any(|r| r.text.contains("某大学") || r.text.contains("经济学院"))
        } else {
            false
        }
    });
    assert!(
        has_institution,
        "complex paper should recover institution info from #align(center)"
    );
}

// ---------------------------------------------------------------------------
// Image embedding tests
// ---------------------------------------------------------------------------

#[test]
fn image_embeds_in_docx() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check image file exists in ZIP
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "should have image in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
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
    let ct_xml = std::io::read_to_string(reader.by_name("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct_xml.contains("image/png"),
        "content types should include image/png"
    );
}

#[test]
fn image_has_relationships_in_rels() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check document rels include image relationship
    let rels_xml =
        std::io::read_to_string(reader.by_name("word/_rels/document.xml.rels").unwrap()).unwrap();
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

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

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

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/image_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

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

// ---------------------------------------------------------------------------
// SVG image rasterization tests
// ---------------------------------------------------------------------------

#[test]
fn svg_image_rasterized_and_embedded() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/svg_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();

    // Check image file exists in ZIP
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(
        names.iter().any(|n| n.starts_with("word/media/image")),
        "SVG should be rasterized to PNG and embedded in word/media/, got: {names:?}"
    );

    // Check document.xml has drawing element
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
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

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/svg_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

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

// ---------------------------------------------------------------------------
// Math P1 element integration tests — new OMML elements
// ---------------------------------------------------------------------------

#[test]
fn math_matrix_produces_m_m() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:m>"),
        "document.xml should contain <m:m> for matrix"
    );
    assert!(
        doc_xml.contains("<m:mr>"),
        "document.xml should contain <m:mr> for matrix row"
    );
}

#[test]
fn math_accent_hat_produces_m_acc() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:acc>"),
        "document.xml should contain <m:acc> for accent"
    );
    assert!(
        doc_xml.contains("<m:accPr>"),
        "document.xml should contain <m:accPr> for accent properties"
    );
    // hat accent should use combining circumflex U+0302
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{0302}\"/>"),
        "hat accent should have chr U+0302 (combining circumflex)"
    );
}

#[test]
fn math_accent_arrow_produces_m_acc_with_arrow_chr() {
    let doc_xml = math_unit_doc_xml();
    // arrow accent should use combining right arrow above U+20D7
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{20D7}\"/>"),
        "arrow accent should have chr U+20D7 (combining right arrow above)"
    );
}

#[test]
fn math_overline_produces_m_bar_top() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:bar>"),
        "document.xml should contain <m:bar> for overline"
    );
    assert!(
        doc_xml.contains("<m:pos m:val=\"top\"/>"),
        "overline should have pos=top"
    );
}

#[test]
fn math_underline_produces_m_bar_bot() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:pos m:val=\"bot\"/>"),
        "underline should have pos=bot"
    );
}

#[test]
fn math_named_func_produces_m_func() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:func>"),
        "document.xml should contain <m:func> for named function"
    );
    assert!(
        doc_xml.contains("<m:fName>"),
        "document.xml should contain <m:fName> for function name"
    );
    // sin should appear as plain-style text
    assert!(
        doc_xml.contains("<m:t>sin</m:t>"),
        "function name should contain 'sin'"
    );
}

#[test]
fn math_cases_produces_m_d_with_m_eqarr() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:eqArr>"),
        "document.xml should contain <m:eqArr> for cases"
    );
    // Cases should have a left brace delimiter
    assert!(
        doc_xml.contains("<m:begChr m:val=\"{\"/>"),
        "cases should have opening brace delimiter"
    );
    // Cases should suppress the closing delimiter
    assert!(
        doc_xml.contains("<m:endChr m:val=\"\"/>"),
        "cases should have empty closing delimiter"
    );
}

#[test]
fn math_underbrace_produces_m_groupchr() {
    let doc_xml = math_unit_doc_xml();
    assert!(
        doc_xml.contains("<m:groupChr>"),
        "document.xml should contain <m:groupChr> for underbrace"
    );
    assert!(
        doc_xml.contains("<m:groupChrPr>"),
        "document.xml should contain <m:groupChrPr> for group char properties"
    );
    // Underbrace uses U+23DF
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{23DF}\"/>"),
        "underbrace should have chr U+23DF (bottom curly bracket)"
    );
}

#[test]
fn math_overbrace_produces_m_groupchr() {
    let doc_xml = math_unit_doc_xml();
    // Overbrace uses U+23DE
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{23DE}\"/>"),
        "overbrace should have chr U+23DE (top curly bracket)"
    );
}

#[test]
fn math_underbrace_annotation_produces_m_limlow() {
    let doc_xml = math_unit_doc_xml();
    // Underbrace with annotation should be wrapped in m:limLow
    assert!(
        doc_xml.contains("<m:limLow>"),
        "underbrace with annotation should produce <m:limLow>"
    );
    assert!(
        doc_xml.contains("<m:lim>"),
        "should have <m:lim> element for the annotation"
    );
}

#[test]
fn math_overbrace_annotation_produces_m_limupp() {
    let doc_xml = math_unit_doc_xml();
    // Overbrace with annotation should be wrapped in m:limUpp
    assert!(
        doc_xml.contains("<m:limUpp>"),
        "overbrace with annotation should produce <m:limUpp>"
    );
}

#[test]
fn math_vector_produces_m_m_in_delimiters() {
    let doc_xml = math_unit_doc_xml();
    // vec() produces a column vector with parentheses and a matrix inside
    // It should have at least 3 m:mr rows (for vec(1, 2, 3))
    let mr_count = doc_xml.matches("<m:mr>").count();
    assert!(
        mr_count >= 3,
        "vector should produce at least 3 <m:mr> rows, got {mr_count}"
    );
}

#[test]
fn math_aligned_equation_produces_standalone_eqarr() {
    let doc_xml = math_unit_doc_xml();
    // The math_unit.typ now has an aligned equation: x &= 1 + 2 \ &= 3
    // This should produce m:eqArr directly inside m:oMath (not wrapped in m:d like cases)
    // Count eqArr occurrences — should be at least 2 (1 from cases + 1 from aligned eq)
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert!(
        eqarr_count >= 2,
        "should have at least 2 <m:eqArr> (cases + aligned equation), got {eqarr_count}"
    );
}

// ---------------------------------------------------------------------------
// Table of Contents (TOC field code) tests
// ---------------------------------------------------------------------------

#[test]
fn toc_produces_field_code() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/toc_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Multi-line aligned equation tests (m:eqArr from AlignPointElem + LinebreakElem)
// ---------------------------------------------------------------------------

fn aligned_equations_doc_xml() -> String {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/aligned_equations.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
}

#[test]
fn aligned_equation_produces_m_eqarr() {
    let doc_xml = aligned_equations_doc_xml();
    // Multi-line aligned equations should produce m:eqArr
    assert!(
        doc_xml.contains("<m:eqArr>"),
        "document.xml should contain <m:eqArr> for aligned equations"
    );
}

#[test]
fn aligned_equation_has_correct_row_count() {
    let doc_xml = aligned_equations_doc_xml();
    // The fixture has:
    //   - Simple alignment: 2 lines (2 m:e)
    //   - Multi-line with expressions: 2 lines (2 m:e)
    //   - Three lines: 3 lines (3 m:e)
    // Total m:e inside eqArr = 7
    // But m:e is also used for other purposes (e.g., delimiters, superscripts),
    // so we count eqArr instances instead.
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert_eq!(
        eqarr_count, 3,
        "should have 3 eqArr elements (one per aligned equation), got {eqarr_count}"
    );
}

#[test]
fn aligned_equation_simple_has_two_rows() {
    let doc_xml = aligned_equations_doc_xml();
    // Find the first m:eqArr and count its direct m:e children
    // The simple alignment "x &= 1 + 2 \ &= 3" should have 2 rows
    if let Some(start) = doc_xml.find("<m:eqArr>") {
        if let Some(end) = doc_xml[start..].find("</m:eqArr>") {
            let eqarr_xml = &doc_xml[start..start + end + "</m:eqArr>".len()];
            let row_count = eqarr_xml.matches("<m:e>").count();
            assert_eq!(
                row_count, 2,
                "simple aligned equation should have 2 rows, got {row_count} in:\n{eqarr_xml}"
            );
        } else {
            panic!("could not find closing </m:eqArr>");
        }
    } else {
        panic!("could not find <m:eqArr> in document.xml");
    }
}

#[test]
fn aligned_equation_three_lines_has_three_rows() {
    let doc_xml = aligned_equations_doc_xml();
    // Find the third m:eqArr (3-line equation: a = b+c, = d+e, = f)
    let mut search_from = 0;
    for _ in 0..2 {
        if let Some(pos) = doc_xml[search_from..].find("<m:eqArr>") {
            search_from += pos + "<m:eqArr>".len();
        } else {
            panic!("could not find enough <m:eqArr> elements");
        }
    }
    // Now find the third one
    if let Some(start_offset) = doc_xml[search_from..].find("<m:eqArr>") {
        let start = search_from + start_offset;
        if let Some(end_offset) = doc_xml[start..].find("</m:eqArr>") {
            let eqarr_xml = &doc_xml[start..start + end_offset + "</m:eqArr>".len()];
            let row_count = eqarr_xml.matches("<m:e>").count();
            assert_eq!(
                row_count, 3,
                "three-line aligned equation should have 3 rows, got {row_count}"
            );
        } else {
            panic!("could not find closing </m:eqArr>");
        }
    } else {
        panic!("could not find third <m:eqArr>");
    }
}

#[test]
fn aligned_equation_contains_alignment_ampersand() {
    let doc_xml = aligned_equations_doc_xml();
    // The alignment point should be emitted as &amp; (XML-escaped ampersand)
    // inside math runs within eqArr
    assert!(
        doc_xml.contains("&amp;"),
        "aligned equations should contain &amp; for alignment points"
    );
}

#[test]
fn aligned_equation_is_wrapped_in_omathpara() {
    let doc_xml = aligned_equations_doc_xml();
    // Block aligned equations should be inside m:oMathPara
    assert!(
        doc_xml.contains("<m:oMathPara>"),
        "block aligned equations should be wrapped in m:oMathPara"
    );
    // Each eqArr should be inside oMathPara > oMath
    // Find a pattern that confirms oMathPara > oMath > eqArr nesting
    let omathpara_count = doc_xml.matches("<m:oMathPara>").count();
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert_eq!(
        omathpara_count, eqarr_count,
        "each eqArr should have a corresponding oMathPara wrapper"
    );
}

// ---------------------------------------------------------------------------
// Headers and footers
// ---------------------------------------------------------------------------

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/header_footer_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/header_footer_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

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
fn columns_produces_w_cols_in_xml() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/columns_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Section breaks
// ---------------------------------------------------------------------------

#[test]
fn section_break_produces_multiple_sect_pr() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/section_break_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/section_break_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Nested lists
// ---------------------------------------------------------------------------

#[test]
fn nested_list_has_multiple_levels() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/nested_list.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/nested_list.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

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

// ---------------------------------------------------------------------------
// Inline formatting tests (super, sub, underline, strike, highlight, smallcaps, raw)
// ---------------------------------------------------------------------------

fn inline_formatting_doc_xml() -> String {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/inline_formatting.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
}

#[test]
fn inline_super_produces_text() {
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
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
    let doc_xml = inline_formatting_doc_xml();
    // SmallcapsElem doesn't have the Tagged trait in Typst 0.14.2, so it won't
    // produce Tag::Start/Tag::End. The text content is preserved but the
    // formatting is not yet applied.  When Typst adds Tagged to SmallcapsElem,
    // the handler will automatically start emitting w:smallCaps.
    assert!(
        doc_xml.contains("Small Caps"),
        "document.xml should preserve the text 'Small Caps' even without formatting"
    );
}

// ---------------------------------------------------------------------------
// Page break detection from PagedDocument
// ---------------------------------------------------------------------------

#[test]
fn pagebreak_inserts_w_br_page() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/pagebreak_test.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Horizontal rule detection from PagedDocument
// ---------------------------------------------------------------------------

#[test]
fn hrule_produces_paragraph_with_bottom_border() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hrule_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hrule_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

// ---------------------------------------------------------------------------
// Math in headings
// ---------------------------------------------------------------------------

#[test]
fn math_in_heading_produces_omml() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/math_in_heading.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("m:oMath"),
        "heading with inline math should produce m:oMath element"
    );
    assert!(doc_xml.contains("Heading2"), "should still be a heading");
}

// ---------------------------------------------------------------------------
// Footnotes inside table cells
// ---------------------------------------------------------------------------

#[test]
fn footnote_in_table_cell_has_reference() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/footnote_in_table.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("w:footnoteReference"),
        "footnote inside table cell should produce w:footnoteReference"
    );

    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();
    assert!(
        fn_xml.contains("inside a table cell"),
        "footnotes.xml should contain the footnote text from the table cell"
    );
}

// ---------------------------------------------------------------------------
// Bug fix: Rowspan generates vMerge continue cells
// ---------------------------------------------------------------------------

#[test]
fn rowspan_produces_vmerge_continue_cells() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/rowspan_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // The first cell of row 0 has rowspan=2 -> vMerge restart
    assert!(
        doc_xml.contains("<w:vMerge w:val=\"restart\"/>"),
        "rowspan start cell should have w:vMerge val=restart"
    );
    // Row 1 should have a vMerge continue cell (empty w:vMerge)
    assert!(
        doc_xml.contains("<w:vMerge/>"),
        "continuation row should have w:vMerge (continue) for the merged cell"
    );
}

#[test]
fn rowspan_all_rows_have_equal_cell_count() {
    use typort_ooxml::document::BlockElement;

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/rowspan_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find the table in the document model
    let table = doc.body.elements.iter().find_map(|e| {
        if let BlockElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });
    let table = table.expect("should have a table");

    // All rows should have the same number of logical columns
    let col_counts: Vec<u32> = table
        .rows
        .iter()
        .map(|r| r.cells.iter().map(|c| c.colspan).sum())
        .collect();
    assert_eq!(
        col_counts.len(),
        3,
        "table should have 3 rows, got {}",
        col_counts.len()
    );
    assert!(
        col_counts.iter().all(|&c| c == col_counts[0]),
        "all rows should have the same logical column count, got: {col_counts:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug fix: Multi-paragraph table cells
// ---------------------------------------------------------------------------

#[test]
fn multi_paragraph_cell_has_multiple_paragraphs() {
    use typort_ooxml::document::BlockElement;

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/multi_para_cell.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find the table in the document model
    let table = doc.body.elements.iter().find_map(|e| {
        if let BlockElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });
    let table = table.expect("should have a table");

    // The second cell (index 1) should have 2 paragraphs
    let row = &table.rows[0];
    assert!(
        row.cells.len() >= 2,
        "first row should have at least 2 cells"
    );
    let multi_cell = &row.cells[1];
    assert!(
        multi_cell.paragraphs.len() >= 2,
        "cell with two paragraphs should have >= 2 Paragraph objects, got {}",
        multi_cell.paragraphs.len()
    );
}

#[test]
fn multi_paragraph_cell_produces_multiple_w_p_in_tc() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/multi_para_cell.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("First paragraph"),
        "should contain first paragraph text"
    );
    assert!(
        doc_xml.contains("Second paragraph"),
        "should contain second paragraph text"
    );
}

// ---------------------------------------------------------------------------
// Bug fix: Footnote content formatting preserved
// ---------------------------------------------------------------------------

#[test]
fn formatted_footnote_preserves_bold_and_italic() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/formatted_footnote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Check the footnote content runs preserve formatting
    assert!(
        !doc.footnotes.is_empty(),
        "should have at least one footnote"
    );
    let fn_content = &doc.footnotes[0].content;
    let has_bold = fn_content.iter().any(|i| {
        matches!(i, typort_ooxml::document::InlineElement::Text(r) if r.bold)
    });
    let has_italic = fn_content.iter().any(|i| {
        matches!(i, typort_ooxml::document::InlineElement::Text(r) if r.italic)
    });
    assert!(has_bold, "footnote content should have a bold run");
    assert!(has_italic, "footnote content should have an italic run");
}

#[test]
fn formatted_footnote_xml_has_formatting_elements() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/formatted_footnote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();

    assert!(
        fn_xml.contains("<w:b/>"),
        "footnotes.xml should contain <w:b/> for bold formatting"
    );
    assert!(
        fn_xml.contains("<w:i/>"),
        "footnotes.xml should contain <w:i/> for italic formatting"
    );
}

// ---------------------------------------------------------------------------
// Bug fix: Bold preserved inside hyperlinks
// ---------------------------------------------------------------------------

#[test]
fn bold_link_preserves_formatting_in_hyperlink() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/bold_link.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/bold_link.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

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

// ---------------------------------------------------------------------------
// Grid layout recovery tests
// ---------------------------------------------------------------------------

#[test]
fn grid_content_recovered_in_output() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/grid_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // All grid text should appear in the output
    assert!(
        doc_xml.contains("Left column text"),
        "document.xml should contain 'Left column text' from grid"
    );
    assert!(
        doc_xml.contains("Right column text"),
        "document.xml should contain 'Right column text' from grid"
    );
    assert!(
        doc_xml.contains("Row 2 left"),
        "document.xml should contain 'Row 2 left' from grid"
    );
    assert!(
        doc_xml.contains("Row 2 right"),
        "document.xml should contain 'Row 2 right' from grid"
    );
    // Normal text after grid should also be present
    assert!(
        doc_xml.contains("Some normal text after grid"),
        "document.xml should contain text after the grid"
    );
}

#[test]
fn grid_multi_column_has_tab_stops() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/grid_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Multi-column grid lines should produce tab stops in the XML
    assert!(
        doc_xml.contains("<w:tab"),
        "grid layout should produce tab elements in the output"
    );
}

#[test]
fn grid_document_model_has_tab_inlines() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/grid_test.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with Tab inline elements (multi-column grid lines)
    let has_tab = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            p.inlines.iter().any(|i| matches!(i, InlineElement::Tab))
        } else {
            false
        }
    });
    assert!(
        has_tab,
        "document model should have Tab inline elements for multi-column grid lines"
    );

    // Paragraphs with Tab should also have tab_stops set
    let has_tab_stops = doc.body.elements.iter().any(|e| {
        if let BlockElement::Paragraph(p) = e {
            !p.tab_stops.is_empty()
        } else {
            false
        }
    });
    assert!(
        has_tab_stops,
        "document model should have tab_stops for multi-column grid lines"
    );
}

// ── Page numbering integration test ────────────────────────────────────

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
fn math_in_table_cells_is_preserved() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/math_in_table.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // The table must exist
    assert!(
        doc_xml.contains("w:tbl"),
        "document.xml should contain a table"
    );

    // Math content must appear inside table cells (inside w:tc elements)
    // Look for OMML math elements that should be generated for $x$, $x^2 + 1$
    assert!(
        doc_xml.contains("<m:oMath>"),
        "document.xml should contain <m:oMath> for inline math in table cells: {doc_xml}"
    );

    // Verify the plain text cells are also present
    assert!(
        doc_xml.contains("Variable"),
        "table should contain 'Variable' header text"
    );
    assert!(
        doc_xml.contains("Formula"),
        "table should contain 'Formula' header text"
    );
    assert!(
        doc_xml.contains("3.14"),
        "table should contain '3.14' value text"
    );
}

#[test]
fn figure_caption_is_single_paragraph() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/figure_caption.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Count how many paragraphs contain parts of the caption text.
    // The caption "Table 1: A simple data table" should NOT be split into
    // separate paragraphs like "Table", "1", ":", "A simple data table".
    // Instead it should be a single paragraph containing all parts.

    // Split document XML on paragraph boundaries to inspect per-paragraph content
    let para_texts: Vec<String> = doc_xml
        .split("<w:p>")
        .skip(1) // skip everything before the first <w:p>
        .map(|p| {
            // Extract text content from <w:t ...>...</w:t> elements within this paragraph
            let mut text = String::new();
            for part in p.split("<w:t") {
                if let Some(rest) = part
                    .strip_prefix(">")
                    .or_else(|| part.find('>').map(|i| &part[i + 1..]))
                {
                    if let Some(end) = rest.find("</w:t>") {
                        text.push_str(&rest[..end]);
                    }
                }
            }
            text
        })
        .collect();

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

// ---- Inline math in text (paragraph splitting regression) ----

/// Helper: generate document.xml for inline_math_in_text.typ fixture.
fn inline_math_in_text_doc_xml() -> String {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/inline_math_in_text.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
}

#[test]
fn inline_math_produces_single_paragraph() {
    let doc_xml = inline_math_in_text_doc_xml();

    // Count <w:p> elements — should be exactly 1 (the single sentence)
    let p_count = doc_xml.matches("<w:p>").count() + doc_xml.matches("<w:p ").count();
    assert_eq!(
        p_count, 1,
        "sentence with inline math should produce exactly 1 paragraph, got {p_count}: {doc_xml}"
    );
}

#[test]
fn inline_math_has_omath_not_omathpara() {
    let doc_xml = inline_math_in_text_doc_xml();

    // Inline math should use <m:oMath>, NOT <m:oMathPara>
    assert!(
        doc_xml.contains("<m:oMath>"),
        "inline math should produce <m:oMath> elements"
    );
    assert!(
        !doc_xml.contains("<m:oMathPara>"),
        "inline math should NOT produce <m:oMathPara> (that's for block equations)"
    );
}

#[test]
fn inline_math_text_runs_are_preserved() {
    let doc_xml = inline_math_in_text_doc_xml();

    // All text fragments should be present
    assert!(
        doc_xml.contains("Where"),
        "text run 'Where' should be present"
    );
    assert!(
        doc_xml.contains("is the dependent variable and"),
        "text run 'is the dependent variable and' should be present"
    );
    assert!(
        doc_xml.contains("is the explanatory variable."),
        "text run 'is the explanatory variable.' should be present"
    );
}

#[test]
fn inline_math_interleaved_with_text_in_same_paragraph() {
    let doc_xml = inline_math_in_text_doc_xml();

    // Find the single <w:p> and verify it contains both text and math
    // by checking that text runs and math elements are siblings inside one <w:p>
    let p_start = doc_xml.find("<w:p>").expect("should have a <w:p>");
    let p_end = doc_xml[p_start..]
        .find("</w:p>")
        .expect("should have </w:p>")
        + p_start;
    let p_content = &doc_xml[p_start..p_end];

    // Should contain both text runs and math
    assert!(
        p_content.contains("<w:r>") && p_content.contains("<m:oMath>"),
        "the single paragraph should contain both <w:r> text runs and <m:oMath> elements"
    );

    // Should contain at least 2 math elements (for $y$ and $x$)
    let math_count = p_content.matches("<m:oMath>").count();
    assert!(
        math_count >= 2,
        "should have at least 2 inline math elements, got {math_count}"
    );
}

// ---------------------------------------------------------------------------
// Visual regression: Typst PDF vs typort docx→PDF pixel comparison
// ---------------------------------------------------------------------------

/// Compile a .typ to PDF via Typst's native renderer (ground truth).
fn typst_to_pdf(typ_path: &Path) -> Vec<u8> {
    let world = typort_core::TyportWorld::new(typ_path).unwrap();
    let paged = typst::compile::<typst::layout::PagedDocument>(&world)
        .output
        .unwrap();
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default()).unwrap()
}

/// Convert .typ → .docx → PDF (via LibreOffice), return PDF bytes.
fn typort_to_pdf_via_docx(typ_path: &Path, label: &str) -> Option<Vec<u8>> {
    let world = typort_core::TyportWorld::new(typ_path).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let tmp_dir = std::env::temp_dir().join("typort_visual_test");
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let docx_path = tmp_dir.join(format!("{label}.docx"));
    let f = std::fs::File::create(&docx_path).ok()?;
    typort_ooxml::write_docx(&doc, std::io::BufWriter::new(f)).ok()?;

    let status = std::process::Command::new("libreoffice")
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&tmp_dir)
        .arg(&docx_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    std::fs::read(tmp_dir.join(format!("{label}.pdf"))).ok()
}

/// Render a PDF page to a PNG image using pdftoppm.
fn pdf_page_to_png(pdf_bytes: &[u8], page: u32, label: &str) -> Option<std::path::PathBuf> {
    let tmp_dir = std::env::temp_dir().join("typort_visual_test");
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let pdf_path = tmp_dir.join(format!("{label}.pdf"));
    std::fs::write(&pdf_path, pdf_bytes).ok()?;
    let out_prefix = tmp_dir.join(format!("{label}_page"));
    let page_str = page.to_string();
    let status = std::process::Command::new("pdftoppm")
        .args(["-png", "-r", "150", "-f", &page_str, "-l", &page_str])
        .arg(&pdf_path)
        .arg(&out_prefix)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let png_name = format!("{label}_page-{page:0>2}.png",);
    let png_path = tmp_dir.join(&png_name);
    if png_path.exists() {
        Some(png_path)
    } else {
        // pdftoppm may use different padding
        let alt = tmp_dir.join(format!("{label}_page-{page}.png"));
        if alt.exists() { Some(alt) } else { None }
    }
}

/// Compare two PNG images using ImageMagick, return the normalized difference (0.0 = identical).
fn compare_images(a: &Path, b: &Path) -> Option<f64> {
    let output = std::process::Command::new("compare")
        .args(["-metric", "RMSE"])
        .arg(a)
        .arg(b)
        .arg("/dev/null")
        .output()
        .ok()?;
    // ImageMagick outputs metric to stderr: "1234.5 (0.0188)"
    let stderr = String::from_utf8_lossy(&output.stderr);
    let paren_start = stderr.find('(')?;
    let paren_end = stderr.find(')')?;
    stderr[paren_start + 1..paren_end].parse::<f64>().ok()
}

#[test]
fn visual_regression_hello() {
    let path = Path::new("../../tests/fixtures/hello.typ");
    let ground_truth = typst_to_pdf(path);
    let Some(docx_pdf) = typort_to_pdf_via_docx(path, "hello") else {
        eprintln!("SKIP: LibreOffice not available for visual regression");
        return;
    };

    let Some(gt_png) = pdf_page_to_png(&ground_truth, 1, "gt_hello") else {
        eprintln!("SKIP: pdftoppm not available");
        return;
    };
    let Some(docx_png) = pdf_page_to_png(&docx_pdf, 1, "docx_hello") else {
        eprintln!("SKIP: pdftoppm failed for docx PDF");
        return;
    };

    if let Some(diff) = compare_images(&gt_png, &docx_png) {
        eprintln!("hello.typ visual diff: {diff:.4} (0=identical, <0.15=acceptable)");
        assert!(
            diff < 0.30,
            "visual regression too high for hello.typ: {diff:.4}"
        );
    }
}

#[test]
fn visual_regression_complex_paper() {
    let path = Path::new("../../tests/fixtures/complex_paper.typ");
    let ground_truth = typst_to_pdf(path);
    let Some(docx_pdf) = typort_to_pdf_via_docx(path, "complex") else {
        eprintln!("SKIP: LibreOffice not available");
        return;
    };

    let Some(gt_png) = pdf_page_to_png(&ground_truth, 1, "gt_complex") else {
        return;
    };
    let Some(docx_png) = pdf_page_to_png(&docx_pdf, 1, "docx_complex") else {
        return;
    };

    if let Some(diff) = compare_images(&gt_png, &docx_png) {
        eprintln!("complex_paper.typ visual diff: {diff:.4}");
        assert!(
            diff < 0.35,
            "visual regression too high for complex_paper.typ: {diff:.4}"
        );
    }
}

// ---------- equation label bookmark tests (#15) ----------

#[test]
fn equation_label_produces_bookmark() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/equation_label.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("w:bookmarkStart") && doc_xml.contains("eq:pythagoras"),
        "document.xml should contain a bookmarkStart with name eq:pythagoras"
    );
    assert!(
        doc_xml.contains("w:bookmarkEnd"),
        "document.xml should contain a bookmarkEnd for the equation bookmark"
    );
}

#[test]
fn equation_label_cross_reference_produces_ref_field() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/equation_label.typ"))
        .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("REF eq:pythagoras"),
        "document.xml should contain a REF field code pointing at eq:pythagoras"
    );
}

// ---------- document title metadata tests (#16) ----------

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
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/centered_heading.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // The heading should be detected as centered from the PagedDocument
    // Look for a Heading1 paragraph with center alignment
    assert!(
        doc_xml.contains(r#"<w:jc w:val="center"/>"#),
        "centered heading should have w:jc center. Got:\n{doc_xml}"
    );
}

#[test]
fn show_rule_colored_bold() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/colored_text.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // The bold text should have red color.
    // Typst's `red` is #ff4136, not #ff0000.
    assert!(
        doc_xml.contains(r#"<w:color w:val="FF4136"/>"#),
        "red bold text should have w:color FF4136. Got:\n{doc_xml}"
    );
}

#[test]
fn large_title_not_split_by_tabs() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/large_title_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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
fn nested_table_produces_nested_w_tbl() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/nested_table_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // There should be 2 w:tbl elements: the outer table and the nested inner table
    let table_count = doc_xml.matches("<w:tbl>").count();
    assert_eq!(
        table_count, 2,
        "should have 2 w:tbl elements (outer + nested), got {table_count}"
    );

    // Both inner cell texts should be present
    assert!(
        doc_xml.contains("Inner A"),
        "nested table should contain 'Inner A'"
    );
    assert!(
        doc_xml.contains("Inner B"),
        "nested table should contain 'Inner B'"
    );

    // Outer cell text should be present
    assert!(
        doc_xml.contains("Outer A"),
        "outer table should contain 'Outer A'"
    );
}

#[test]
fn nested_table_document_model_has_cell_content() {
    use typort_ooxml::document::{BlockElement, CellContent};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/nested_table_test.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find the table in the document model
    let table = doc.body.elements.iter().find_map(|e| {
        if let BlockElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });
    assert!(table.is_some(), "document should contain a table");

    let table = table.unwrap();
    assert_eq!(table.rows.len(), 1, "outer table should have 1 row");
    assert_eq!(
        table.rows[0].cells.len(),
        2,
        "outer table row should have 2 cells"
    );

    // Second cell should have nested table content
    let cell_with_nested = &table.rows[0].cells[1];
    let has_nested_table = cell_with_nested
        .content
        .iter()
        .any(|c| matches!(c, CellContent::Table(_)));
    assert!(
        has_nested_table,
        "second cell should have a nested table in its content"
    );
}

// ---------------------------------------------------------------------------
// Show rule style recovery tests
// ---------------------------------------------------------------------------

#[test]
fn show_rule_heading_font_and_size() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/show_rule_styles.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

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

    // Heading size should be 18pt = 36 half-points (from show rule)
    assert!(
        doc_xml.contains(r#"<w:sz w:val="36"/>"#),
        "heading should have size 36 half-points (18pt) from show rule. Got:\n{doc_xml}"
    );
}

#[test]
fn show_rule_bold_size_override() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/show_rule_styles.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Bold text should have 14pt = 28 half-points (from show rule: set text(size: 14pt))
    assert!(
        doc_xml.contains(r#"<w:sz w:val="28"/>"#),
        "bold text should have size 28 half-points (14pt) from show rule. Got:\n{doc_xml}"
    );
}

#[test]
fn show_rule_italic_color() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/show_rule_styles.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    // Italic text should be blue (from show rule: set text(fill: rgb("#0000FF")))
    assert!(
        doc_xml.contains(r#"<w:color w:val="0000FF"/>"#),
        "italic text should have blue color from show rule. Got:\n{doc_xml}"
    );
}

// ── Edge case: separate numbered lists restart numbering ─────────────

#[test]
fn edge_list_restart_separate_lists_get_unique_num_ids() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_list_restart.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    let num_xml =
        std::io::read_to_string(reader.by_name("word/numbering.xml").unwrap()).unwrap();

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

// ── Edge case: blockquote has non-zero left indent ───────────────────

#[test]
fn edge_blockquote_has_left_indent() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_blockquote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("w:ind w:left=\""),
        "blockquote paragraphs should have w:ind w:left set"
    );
    assert!(
        !doc_xml.contains("w:ind w:left=\"0\""),
        "blockquote indent should not be zero"
    );
}

// ── Edge case: math equations preserved in footnotes ─────────────────

#[test]
fn edge_math_in_footnote_preserved() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_math_in_footnote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let fn_xml =
        std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();

    assert!(
        fn_xml.contains("m:oMath"),
        "footnotes.xml should contain m:oMath elements for math in footnotes"
    );
}

// ── Edge case: super/subscript preserved in headings ─────────────────

#[test]
fn edge_super_sub_in_heading_preserved() {
    let world = typort_core::TyportWorld::new(Path::new(
        "../../tests/fixtures/edge_super_sub_in_heading.typ",
    ))
    .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(
        doc_xml.contains("w:vertAlign w:val=\"subscript\""),
        "heading with H₂O should have subscript vertAlign"
    );
    assert!(
        doc_xml.contains("w:vertAlign w:val=\"superscript\""),
        "heading with x² should have superscript vertAlign"
    );
}

// ── Issue fixtures: competitor bug regression tests ─────────────────

fn issue_doc_xml(fixture: &str) -> String {
    let path = format!("../../tests/fixtures/{fixture}.typ");
    let world = typort_core::TyportWorld::new(Path::new(&path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
}

#[test]
fn issue_cjk_linebreak_no_spurious_spaces() {
    let xml = issue_doc_xml("issue_cjk_linebreak");
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
    let xml = issue_doc_xml("issue_context_equation");
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
    let xml = issue_doc_xml("issue_figure_caption");
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
fn issue_inline_math_spacing_preserved() {
    let xml = issue_doc_xml("issue_inline_math_spacing");
    assert!(
        xml.contains("<m:oMath>"),
        "inline math should produce OMML elements"
    );
    assert!(
        xml.contains("Let"),
        "text 'Let' should be present"
    );
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
    let xml = issue_doc_xml("issue_mat_delimiter");
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
    let xml = issue_doc_xml("issue_rotate_content");
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
    let xml = issue_doc_xml("issue_show_rule_heading");
    assert!(
        xml.contains("Main Title"),
        "heading 1 text should be present"
    );
    assert!(
        xml.contains("Blue Subtitle"),
        "heading 2 text should be present"
    );
    assert!(
        xml.contains("w:val=\"36\"") || xml.contains("w:val=\"35\""),
        "heading 1 should have ~18pt size (36 half-pts)"
    );
    assert!(
        xml.contains("Heading1"),
        "heading 1 should use Heading1 style"
    );
}

#[test]
fn issue_smart_quotes_preserved() {
    let xml = issue_doc_xml("issue_smart_quotes");
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
    let xml = issue_doc_xml("issue_linebreak_heading");
    assert!(
        xml.contains("Heading with"),
        "heading text before line break should be present"
    );
    assert!(
        xml.contains("line break"),
        "heading text after line break should be present"
    );
    assert!(
        xml.contains("Heading1"),
        "should have Heading1 style"
    );
}

#[test]
fn issue_display_math_in_list_numbering() {
    let xml = issue_doc_xml("issue_display_math_in_list");
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
    let xml = issue_doc_xml("issue_nested_enum_reset");
    for text in ["Parent A", "Parent B", "Parent C", "Sub one", "Sub two", "Sub one again"] {
        assert!(
            xml.contains(text),
            "nested enum should contain '{text}'"
        );
    }
}

#[test]
fn issue_subscript_scope_omml() {
    let xml = issue_doc_xml("issue_subscript_scope");
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
    let xml = issue_doc_xml("issue_tight_list_sublist");
    let item1_count = xml.matches("Item 1").count();
    assert_eq!(
        item1_count, 1,
        "'Item 1' should appear exactly once (no recovery duplication), got {item1_count}"
    );
    assert!(
        xml.contains("Sub-item A"),
        "sub-item should be present"
    );
}

#[test]
fn issue_heading_numbering_correct_order() {
    let xml = issue_doc_xml("issue_heading_numbering_show");
    let intro_pos = xml.find("Introduction").expect("Introduction should exist");
    let bg_pos = xml.find("Background").expect("Background should exist");
    let methods_pos = xml.find("Methods").expect("Methods should exist");
    assert!(
        intro_pos < bg_pos && bg_pos < methods_pos,
        "headings should appear in order: Introduction < Background < Methods"
    );
}
