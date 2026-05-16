use std::io::Cursor;
use std::path::Path;

#[test]
fn complex_paper_has_table_structure() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

    // Verify footnotes were detected in the document model
    assert!(!doc.footnotes.is_empty(), "should have detected footnotes");
    assert_eq!(doc.footnotes.len(), 3, "complex paper has 3 footnotes");

    // Verify footnote content was extracted
    let first_fn_text: String = doc.footnotes[0]
        .content
        .iter()
        .map(|r| r.text.as_str())
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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

    assert_eq!(
        doc.metadata.title.as_deref(),
        Some("Hello World"),
        "metadata title should be extracted from first heading"
    );
}

#[test]
fn preset_overrides_page_margins() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let mut doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
    let doc = typort_core::convert_html(&world).unwrap();

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
        colspan: 2,
        vmerge: VMerge::Restart,
    };
    assert_eq!(cell.colspan, 2);
    assert_eq!(cell.vmerge, VMerge::Restart);

    // Verify VMerge::Continue
    let cont_cell = TableCell {
        paragraphs: vec![Paragraph::new()],
        colspan: 1,
        vmerge: VMerge::Continue,
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
                        colspan: 2,
                        vmerge: VMerge::Restart,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        colspan: 1,
                        vmerge: VMerge::None,
                    },
                ],
            },
            TableRow {
                cells: vec![
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        colspan: 2,
                        vmerge: VMerge::Continue,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        colspan: 1,
                        vmerge: VMerge::None,
                    },
                ],
            },
        ],
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
