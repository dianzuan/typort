//! Table structure, cell merging, and math-in-table tests.

use crate::common::{
    fixture_doc_xml, fixture_document, fixture_package, fixture_package_from_document,
    paragraph_containing,
};

#[test]
fn three_line_table_is_not_a_boxed_grid() {
    // Regression: a three-line table was emitted as a full grid. See
    // the `edge_three_line_table` fixture.
    let doc_xml = fixture_doc_xml("edge_three_line_table");
    let tbl_start = doc_xml.find("<w:tbl>").expect("table present");
    let tbl_end = doc_xml[tbl_start..]
        .find("</w:tbl>")
        .map(|e| tbl_start + e)
        .expect("table closed");
    let table = &doc_xml[tbl_start..tbl_end];

    // No vertical, inner-horizontal, or side grid lines.
    assert!(
        table.contains(r#"<w:insideV w:val="nil"/>"#),
        "three-line table must suppress vertical rules"
    );
    assert!(
        table.contains(r#"<w:insideH w:val="nil"/>"#),
        "three-line table must suppress inner-row rules"
    );
    assert!(
        table.contains(r#"<w:left w:val="nil"/>"#) && table.contains(r#"<w:right w:val="nil"/>"#),
        "three-line table must have no left/right rules"
    );
    // Top and bottom rules are present.
    assert!(
        table.contains(r#"<w:top w:val="single""#) && table.contains(r#"<w:bottom w:val="single""#),
        "three-line table must keep top and bottom rules"
    );
    // Header separator: a bottom border on the header row's cells.
    assert!(
        table.contains("<w:tcBorders>"),
        "three-line table must draw a separator under the header row"
    );
}

#[test]
fn table_cell_inline_math_is_spliced_not_dropped() {
    // Regression: inline equations inside table cells are `equation` Tag siblings
    // between the cell's <p> text fragments. convert_cell_paragraphs only consumed
    // the <p>s, dropping the math and stacking mixed text+math cells into separate
    // paragraphs. See the `table_cell_math` fixture.
    let doc_xml = fixture_doc_xml("table_cell_math");

    // The only math in the fixture lives inside the table, so its OMML must show
    // up within the <w:tbl> block.
    let tbl_start = doc_xml
        .find("<w:tbl>")
        .expect("document should contain a table");
    let tbl_end = doc_xml[tbl_start..]
        .find("</w:tbl>")
        .map(|e| tbl_start + e)
        .expect("table should be closed");
    let table_xml = &doc_xml[tbl_start..tbl_end];

    // The fixture has 4 inline equations in cells: bold(e)_1, M, times, v*(M).
    // The math-only cell already worked; the regression is the mixed text+math
    // cell, whose equation siblings were dropped — so require all 4.
    let omml_count = table_xml.matches("<m:oMath>").count();
    assert!(
        omml_count >= 4,
        "all 4 cell equations should be spliced as OMML, got {omml_count}"
    );
    // bold(e)_1 -> 𝒆 (U+1D486) must survive inside the table.
    assert!(
        table_xml.contains('\u{1D486}'),
        "bold(e)_1 in a cell should render as 𝒆"
    );
    // The mixed text+math cell ($M$分布 $times$ $v^*(M)$) must keep its math in the
    // same cell/paragraph as the text "分布", not drop it.
    let mixed_cell = table_xml
        .match_indices("分布")
        .find_map(|(pos, _)| {
            let start = table_xml[..pos].rfind("<w:tc>")?;
            let end = table_xml[pos..].find("</w:tc>").map(|e| pos + e)?;
            Some(&table_xml[start..end])
        })
        .expect("a cell containing 分布 should exist");
    assert!(
        mixed_cell.contains("<m:oMath>"),
        "the mixed text+math cell must keep its inline math, not drop it"
    );
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
        vertical_align: None,
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
        vertical_align: None,
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
                        vertical_align: None,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 1,
                        vmerge: VMerge::None,
                        width_pct: None,
                        vertical_align: None,
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
                        vertical_align: None,
                    },
                    TableCell {
                        paragraphs: vec![Paragraph::new()],
                        content: Vec::new(),
                        colspan: 1,
                        vmerge: VMerge::None,
                        width_pct: None,
                        vertical_align: None,
                    },
                ],
            },
        ],
        width_pct: None,
        border_size: None,
        borders: None,
    };
    doc.add_table(table);

    let package = fixture_package_from_document(&doc);
    let doc_xml = package.part_text("word/document.xml");

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

#[test]
fn footnote_in_table_cell_has_reference() {
    let package = fixture_package("footnote_in_table");
    let doc_xml = package.part_text("word/document.xml");

    assert!(
        doc_xml.contains("w:footnoteReference"),
        "footnote inside table cell should produce w:footnoteReference"
    );

    let fn_xml = package.part_text("word/footnotes.xml");
    assert!(
        fn_xml.contains("inside a table cell"),
        "footnotes.xml should contain the footnote text from the table cell"
    );
}

#[test]
fn rowspan_produces_vmerge_continue_cells() {
    let doc_xml = fixture_doc_xml("rowspan_test");

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

    let doc = fixture_document("rowspan_test");

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

#[test]
fn multi_paragraph_cell_has_multiple_paragraphs() {
    use typort_ooxml::document::BlockElement;

    let doc = fixture_document("multi_para_cell");

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
    let doc_xml = fixture_doc_xml("multi_para_cell");

    assert!(
        doc_xml.contains("First paragraph"),
        "should contain first paragraph text"
    );
    assert!(
        doc_xml.contains("Second paragraph"),
        "should contain second paragraph text"
    );
}

#[test]
fn math_in_table_cells_is_preserved() {
    let doc_xml = fixture_doc_xml("math_in_table");

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
fn nested_table_produces_nested_w_tbl() {
    let doc_xml = fixture_doc_xml("nested_table_test");

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

    let doc = fixture_document("nested_table_test");

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

#[test]
fn fr_column_tracks_produce_proportional_widths() {
    // Regression: `columns: (1fr, 2fr, 3fr)` must yield a 1:2:3 width split, not
    // three equal columns. The writer falls back to equal distribution unless
    // cell.width_pct is populated from the Typst column track sizes.
    // See the `edge_table_fr_columns` fixture.
    let doc_xml = fixture_doc_xml("edge_table_fr_columns");
    let tbl_start = doc_xml.find("<w:tbl>").expect("table present");
    let tbl_end = doc_xml[tbl_start..]
        .find("</w:tbl>")
        .map(|e| tbl_start + e)
        .expect("table closed");
    let table = &doc_xml[tbl_start..tbl_end];

    // Parse the first row's three w:tcW percentages.
    let row_end = table.find("</w:tr>").expect("first row closed");
    let first_row = &table[..row_end];
    let widths: Vec<u32> = first_row
        .match_indices("<w:tcW w:w=\"")
        .filter_map(|(pos, m)| {
            let after = &first_row[pos + m.len()..];
            let end = after.find('"')?;
            after[..end].parse::<u32>().ok()
        })
        .collect();

    assert_eq!(
        widths.len(),
        3,
        "expected three column widths, got {widths:?}"
    );
    // NOT the equal-distribution bug (1666 / 1666 / 1666).
    assert!(
        widths[0] < widths[1] && widths[1] < widths[2],
        "1fr:2fr:3fr widths must strictly increase, got {widths:?}"
    );
    assert!(
        (790..=880).contains(&widths[0]),
        "col0 ~833, got {}",
        widths[0]
    );
    assert!(
        (1600..=1730).contains(&widths[1]),
        "col1 ~1666, got {}",
        widths[1]
    );
    assert!(
        (2420..=2580).contains(&widths[2]),
        "col2 ~2500, got {}",
        widths[2]
    );
}

#[test]
fn table_cell_alignment_from_typst_reaches_word() {
    // Faithful (non-hardcoded) table conversion: the cell alignment the author set
    // in Typst must reach Word, read from the semantic TableElem (the HTML export
    // drops it). Vertical alignment -> <w:vAlign> in tcPr; horizontal -> <w:jc> in
    // the cell paragraph (Pandoc-style). See the `edge_table_cell_alignment` fixture.
    let doc_xml = fixture_doc_xml("edge_table_cell_alignment");
    let tc_blocks: Vec<&str> = doc_xml
        .split("<w:tc>")
        .skip(1)
        .map(|c| c.split("</w:tc>").next().unwrap_or(""))
        .collect();
    let merged = tc_blocks
        .iter()
        .find(|b| b.contains("Merged"))
        .expect("Merged cell present");
    assert!(
        merged.contains(r#"<w:vAlign w:val="center"/>"#),
        "the align:horizon merged cell must carry <w:vAlign center> in its tcPr:\n{merged}"
    );
    let r1 = tc_blocks
        .iter()
        .find(|b| b.contains("R1"))
        .expect("R1 cell present");
    assert!(
        r1.contains(r#"<w:jc w:val="right"/>"#),
        "the align:right cell must carry <w:jc right> in its paragraph:\n{r1}"
    );
}

#[test]
fn table_cell_alignment_closure_reaches_word() {
    let doc_xml = fixture_doc_xml("table_cell_alignment_closure");
    let cells: Vec<&str> = doc_xml
        .split("<w:tc>")
        .skip(1)
        .map(|cell| cell.split("</w:tc>").next().unwrap_or(""))
        .collect();
    let left = cells
        .iter()
        .find(|cell| cell.contains("Closure left"))
        .expect("left closure cell present");
    let right = cells
        .iter()
        .find(|cell| cell.contains("Closure right"))
        .expect("right closure cell present");

    assert!(
        left.contains(r#"<w:jc w:val="left"/>"#),
        "the closure's x=0 result must reach the left cell:\n{left}"
    );
    assert!(
        right.contains(r#"<w:jc w:val="right"/>"#),
        "the closure's x=1 result must reach the right cell:\n{right}"
    );
}

#[test]
fn table_cells_do_not_inherit_body_first_line_indent() {
    // Table cells are their own context — they must not take the body's
    // first-line indent (which they would inherit from the Normal style).
    let doc_xml = fixture_doc_xml("complex_paper");
    let tbl_start = doc_xml.find("<w:tbl>").expect("table present");
    let tbl_end = doc_xml[tbl_start..]
        .find("</w:tbl>")
        .map_or(doc_xml.len(), |e| tbl_start + e);
    let table = &doc_xml[tbl_start..tbl_end];
    // Every cell paragraph suppresses the indent (firstLine="0"). complex_paper
    // declares an em-based first-line indent, so the Normal style carries
    // `firstLineChars`; the cell override must zero BOTH the char-based and the
    // twips value, else Word's char-based indent would win.
    let cell_paras = table.matches("<w:p>").count();
    let suppressed = table
        .matches(r#"<w:ind w:firstLineChars="0" w:firstLine="0"/>"#)
        .count();
    assert!(
        suppressed >= cell_paras && cell_paras > 0,
        "all {cell_paras} cell paragraphs must suppress first-line indent, got {suppressed}"
    );
}

#[test]
fn nested_table_cell_keeps_paged_styles() {
    let doc_xml = fixture_doc_xml("nested_table_cell_style");
    let marker_para = paragraph_containing(&doc_xml, "RED-NESTED-MARKER");
    assert!(
        marker_para.contains("<w:color"),
        "rendering-detected color must survive in nested-table cell content:\n{marker_para}"
    );
}

#[test]
fn table_borders_decided_per_table() {
    let doc_xml = fixture_doc_xml("mixed_table_borders");
    let tables: Vec<&str> = {
        let mut out = Vec::new();
        let mut rest = doc_xml.as_str();
        while let Some(start) = rest.find("<w:tbl>") {
            let end = rest[start..].find("</w:tbl>").map(|e| start + e).unwrap();
            out.push(&rest[start..end]);
            rest = &rest[end..];
        }
        out
    };
    assert_eq!(tables.len(), 2, "fixture has two top-level tables");

    let borderless = tables[0];
    let borders_block = {
        let start = borderless
            .find("<w:tblBorders>")
            .expect("tblBorders present");
        let end = borderless
            .find("</w:tblBorders>")
            .expect("tblBorders closed");
        &borderless[start..end]
    };
    assert!(
        !borders_block.contains("w:val=\"single\""),
        "stroke:none table must not gain invented borders:\n{borders_block}"
    );

    let three_line = tables[1];
    assert!(
        three_line.contains("<w:top w:val=\"single\" w:sz=\"8\"")
            && three_line.contains("<w:bottom w:val=\"single\" w:sz=\"8\""),
        "three-line table keeps its own 1pt outer rules:\n{three_line}"
    );
    assert!(
        !three_line.contains("<w:insideV w:val=\"single\""),
        "three-line table must not gain vertical borders"
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
fn issue_table_hline_border_structure() {
    let xml = fixture_doc_xml("issue_table_hline_border");
    assert!(xml.contains("Column A"), "header cell A should be present");
    assert!(xml.contains("Column B"), "header cell B should be present");
    assert!(xml.contains("Data 1"), "data cell should be present");
    assert!(xml.contains("<w:tbl>"), "table should be present");
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
fn issue_table_compact_style_override_content() {
    let xml = fixture_doc_xml("issue_table_compact_style_override");
    assert!(xml.contains("<w:tbl>"), "table should be present");
    assert!(
        xml.contains("<w:b/>"),
        "bold text in table should be preserved"
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
