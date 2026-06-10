use std::io::Cursor;
use std::path::Path;

/// Return the single `<w:p>...</w:p>` block that contains `needle`.
fn paragraph_containing<'a>(doc_xml: &'a str, needle: &str) -> &'a str {
    let pos = doc_xml
        .find(needle)
        .unwrap_or_else(|| panic!("document should contain {needle:?}"));
    let start = doc_xml[..pos]
        .rfind("<w:p>")
        .or_else(|| doc_xml[..pos].rfind("<w:p "))
        .expect("paragraph start");
    let end = doc_xml[pos..]
        .find("</w:p>")
        .map(|e| pos + e)
        .expect("paragraph end");
    &doc_xml[start..end]
}

#[test]
fn inline_math_spacing_cjk_tight_latin_spaced() {
    // Regression: the inline-equation merge must not insert a literal space between
    // CJK text and an equation (Typst renders 标量M tight); it must keep the space
    // for Latin text (Typst trims it, Word needs it back). See
    // tests/fixtures/edge_cjk_inline_math_spacing.typ.
    //
    // The run-coalescing post-pass folds the (formerly standalone) space run into
    // the adjacent text run, so we assert the space is present *in the neighbouring
    // run's text* (tight for CJK, spaced for Latin) rather than as its own run.
    let doc_xml = fixture_doc_xml("edge_cjk_inline_math_spacing");

    let cjk = paragraph_containing(&doc_xml, "标量");
    assert!(
        cjk.contains("标量</w:t>") && !cjk.contains("标量 "),
        "CJK text adjacent to inline math must stay tight (no inserted space):\n{cjk}"
    );
    let latin = paragraph_containing(&doc_xml, "the value");
    assert!(
        latin.contains("the value </w:t>") && latin.contains(" is here."),
        "Latin text around inline math must keep its space:\n{latin}"
    );
}

#[test]
fn par_wrapped_inline_math_keeps_prose_with_math() {
    // Regression: prose inside an author par()[...] wrapper around inline math was
    // dropped (only the equations survived as an orphan math paragraph). See
    // tests/fixtures/edge_par_wraps_inline_math.typ.
    let doc_xml = fixture_doc_xml("edge_par_wraps_inline_math");

    // The prose around the inline math must survive — especially the text AFTER
    // the last equation, which the skip dropped entirely.
    assert!(
        doc_xml.contains("Lead") && doc_xml.contains("matters here"),
        "wrapped-par prose must not be dropped"
    );
    // The prose must sit in the SAME paragraph as its OMML, not be split off into
    // an orphan math-only paragraph.
    let lead_para = paragraph_containing(&doc_xml, "Lead");
    assert!(
        lead_para.contains("<m:oMath>"),
        "wrapped-par prose and its inline math must share one paragraph"
    );
    assert!(
        lead_para.contains("matters here"),
        "prose after the last inline equation must stay in the same paragraph"
    );
    // No recovery-injected duplicate of the prose.
    assert_eq!(
        doc_xml.matches("matters here").count(),
        1,
        "wrapped-par prose must appear exactly once"
    );
    // No-regression: a flat body paragraph with inline math also interleaves prose
    // and OMML in one paragraph (the existing merge path must be unaffected).
    let body_para = paragraph_containing(&doc_xml, "Flat body paragraph");
    assert!(
        body_para.contains("<m:oMath>"),
        "flat body paragraph must keep prose and inline math together"
    );
}

#[test]
fn three_line_table_is_not_a_boxed_grid() {
    // Regression: a three-line table was emitted as a full grid. See
    // tests/fixtures/edge_three_line_table.typ.
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
fn long_left_heading_not_misclassified_as_centered() {
    // Regression: a long left-aligned heading whose text spans most of the line
    // has a text-center near the page center and was wrongly marked centered. See
    // tests/fixtures/edge_long_left_heading.typ.
    let doc_xml = fixture_doc_xml("edge_long_left_heading");
    assert!(
        !doc_xml.contains(r#"<w:jc w:val="center"/>"#),
        "long left-aligned headings must not be misclassified as centered:\n{doc_xml}"
    );
}

#[test]
fn recovery_does_not_inject_citation_or_duplicate_orphans() {
    // Regression for recover_missing_content (recovery.rs): paged body lines whose
    // prose is broken up by OMML math and superscript citations used to be misjudged
    // as "missing" and prepended at body index 0, injecting citation-number strings
    // and duplicated body sentences as orphans above the abstract. See
    // tests/fixtures/edge_recovery_no_orphans.typ.
    let doc_xml = fixture_doc_xml("edge_recovery_no_orphans");

    // Collect each paragraph's plain text (w:t only).
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

    // No paragraph may consist solely of citation/footnote markers like "[1]".
    let is_marker_only = |t: &str| {
        let t = t.trim();
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '[' | ']' | ',' | ' '))
            && t.contains('[')
    };
    assert!(
        !para_texts.iter().any(|t| is_marker_only(t)),
        "recovery must not inject citation-marker-only orphan paragraphs: {para_texts:?}"
    );

    // The abstract must sit right after the title block, not be pushed below
    // injected orphans.
    let abstract_idx = para_texts
        .iter()
        .position(|t| t.contains("摘要"))
        .expect("abstract present");
    assert!(
        abstract_idx <= 2,
        "abstract should stay near the top (after title block), found at {abstract_idx}"
    );

    // Body sentences must appear exactly once (no recovery-injected duplicate).
    assert_eq!(
        doc_xml.matches("正文第一段").count(),
        1,
        "body sentence must not be duplicated by recovery"
    );
}

#[test]
fn table_cell_inline_math_is_spliced_not_dropped() {
    // Regression: inline equations inside table cells are `equation` Tag siblings
    // between the cell's <p> text fragments. convert_cell_paragraphs only consumed
    // the <p>s, dropping the math and stacking mixed text+math cells into separate
    // paragraphs. See tests/fixtures/table_cell_math.typ.
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
fn complex_paper_has_table_structure() {
    let doc_xml = fixture_doc_xml("complex_paper");

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
    let doc_xml = fixture_doc_xml("complex_paper");

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
fn math_test_produces_omml() {
    let doc_xml = fixture_doc_xml("math_test");

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
fn math_styled_wrappers_and_dif_are_not_dropped() {
    // Regression: bold()/bb()/cal() and the upright differential `dif` used to
    // fall into convert_content's silent "unknown element" skip, producing empty
    // <m:e> bases and vanishing glyphs. See tests/fixtures/math_styled_and_dif.typ.
    let doc_xml = fixture_doc_xml("math_styled_and_dif");
    let packed: String = doc_xml.chars().filter(|c| !c.is_whitespace()).collect();

    // bb(R) -> blackboard-bold ℝ (U+211D), used twice in the fixture.
    assert!(
        doc_xml.contains('\u{211D}'),
        "bb(R) should render as blackboard-bold ℝ, not be dropped"
    );
    // bold(e) -> mathematical bold-italic 𝒆 (U+1D486).
    assert!(
        doc_xml.contains('\u{1D486}'),
        "bold(e) should render as bold 𝒆, not be dropped"
    );
    // bold(s) -> 𝒔 (U+1D494, mathematical bold-italic small s).
    assert!(
        doc_xml.contains('\u{1D494}'),
        "bold(s) should render as bold 𝒔, not be dropped"
    );
    // No styled atom may leave an empty math base behind.
    assert!(
        !packed.contains("<m:e></m:e>"),
        "styled math atoms must not leave empty <m:e> bases"
    );
    // dif = upright(d): forced non-italic via an explicit m:sty value "p".
    assert!(
        doc_xml.contains("<m:sty m:val=\"p\"/>"),
        "upright(d) (dif) should force an upright run via m:sty p"
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
    let doc_xml = fixture_doc_xml("numbered_eq");

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
    // Word-native numbered equation: a center tab centers the math and a right tab
    // holds the number, around inline <m:oMath> (not a standalone block oMathPara,
    // which cannot share a line with the trailing number).
    assert!(
        doc_xml.contains(r#"<w:tab w:val="center""#),
        "numbered equation should be centered with a center tab stop"
    );
    assert!(
        doc_xml.contains(r#"<w:tab w:val="right""#),
        "numbered equation should hold the number at a right tab stop"
    );
    assert!(
        doc_xml.contains("<m:oMath>"),
        "document.xml should contain the equation as inline OMML"
    );
    // All equations here are numbered, so none should be a standalone block.
    assert!(
        !doc_xml.contains("<m:oMathPara>"),
        "a numbered equation must not be a standalone block oMathPara"
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
        borders: None,
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
fn math_nary_operand_is_inside_m_e() {
    // The summand must live inside the n-ary's <m:e>, not be left empty with the
    // operand detached as siblings (ECMA-376 violation). For `sum_(i=1)^n i^2`,
    // the m:e following m:sup must contain the summand `i^2` (an m:sSup).
    let xml = fixture_doc_xml("issue_nary_operand_body");
    let start = xml.find("<m:nary>").expect("expected an n-ary for the sum");
    let end = xml.find("</m:nary>").expect("n-ary should close");
    let nary = &xml[start..end];
    assert!(
        !nary.contains("</m:sup><m:e/>") && !nary.contains("</m:sup><m:e></m:e>"),
        "n-ary body <m:e> must not be empty: {nary}"
    );
    // The summand i^2 (an m:sSup) lives inside the n-ary body.
    assert!(
        nary.contains("<m:sSup>"),
        "summand i^2 should be inside the n-ary body, got: {nary}"
    );
}

#[test]
fn math_nary_operand_stops_at_relation() {
    // Operand boundary = a Relation-class symbol. In `sum_(i=1)^n i^2 = S`, the
    // `= S` must fall OUTSIDE the n-ary's operand body, not get pulled into the
    // summand. (Note the `=` inside the lower limit `i=1` is in <m:sub> and is
    // legitimately within the n-ary — so we check the operand <m:e> specifically,
    // not the whole n-ary.) The fixture has exactly one n-ary.
    let xml = fixture_doc_xml("issue_nary_operand_body");
    let nary_end = xml.find("</m:nary>").expect("expected an n-ary");
    let nary = &xml[..nary_end];
    // The operand body is the <m:e> that follows </m:sup>.
    let body_start = nary.find("</m:sup>").expect("n-ary has an upper limit") + "</m:sup>".len();
    let operand = &nary[body_start..];
    assert!(
        !operand.contains("<m:t>=</m:t>"),
        "the `=` relation must NOT be inside the n-ary operand body: {operand}"
    );
    let after = &xml[nary_end..];
    assert!(
        after.contains("<m:t>=</m:t>") && after.contains("<m:t>S</m:t>"),
        "the `= S` must appear after </m:nary>"
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

// ---------------------------------------------------------------------------
// Section breaks
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Nested lists
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Horizontal rule detection from PagedDocument
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Math in headings
// ---------------------------------------------------------------------------

#[test]
fn math_in_heading_produces_omml() {
    let doc_xml = fixture_doc_xml("math_in_heading");

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
    let doc_xml = fixture_doc_xml("grid_test");

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
    let doc_xml = fixture_doc_xml("grid_test");

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
fn figure_caption_is_single_paragraph() {
    let doc_xml = fixture_doc_xml("figure_caption");

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

// The visual-regression tests render typort's docx to PDF (LibreOffice), to PNG
// (pdftoppm), and RMSE-compare against Typst's own PDF (ImageMagick). They are
// `#[ignore]`d because those tools are not in CI — but, when opted into with
// `cargo test -- --ignored`, a MISSING tool is a hard `panic!`, never a silent
// pass, so "ran but skipped" can no longer masquerade as "passed".
#[test]
#[ignore = "needs libreoffice + pdftoppm + ImageMagick; run with --ignored"]
fn visual_regression_hello() {
    let path = Path::new("../../tests/fixtures/hello.typ");
    let ground_truth = typst_to_pdf(path);
    let docx_pdf = typort_to_pdf_via_docx(path, "hello")
        .expect("libreoffice required: install it or do not opt into the --ignored visual tests");
    let gt_png =
        pdf_page_to_png(&ground_truth, 1, "gt_hello").expect("pdftoppm required for ground truth");
    let docx_png =
        pdf_page_to_png(&docx_pdf, 1, "docx_hello").expect("pdftoppm required for docx render");
    let diff = compare_images(&gt_png, &docx_png).expect("ImageMagick `compare` required");
    eprintln!("hello.typ visual diff: {diff:.4} (0=identical, <0.15=acceptable)");
    assert!(
        diff < 0.30,
        "visual regression too high for hello.typ: {diff:.4}"
    );
}

#[test]
#[ignore = "needs libreoffice + pdftoppm + ImageMagick; run with --ignored"]
fn visual_regression_complex_paper() {
    let path = Path::new("../../tests/fixtures/complex_paper.typ");
    let ground_truth = typst_to_pdf(path);
    let docx_pdf = typort_to_pdf_via_docx(path, "complex")
        .expect("libreoffice required: install it or do not opt into the --ignored visual tests");
    let gt_png = pdf_page_to_png(&ground_truth, 1, "gt_complex")
        .expect("pdftoppm required for ground truth");
    let docx_png =
        pdf_page_to_png(&docx_pdf, 1, "docx_complex").expect("pdftoppm required for docx render");
    let diff = compare_images(&gt_png, &docx_png).expect("ImageMagick `compare` required");
    eprintln!("complex_paper.typ visual diff: {diff:.4}");
    assert!(
        diff < 0.35,
        "visual regression too high for complex_paper.typ: {diff:.4}"
    );
}

// ---------- equation label bookmark tests (#15) ----------

#[test]
fn equation_label_produces_bookmark() {
    let doc_xml = fixture_doc_xml("equation_label");

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
    let doc_xml = fixture_doc_xml("equation_label");

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
    let num_xml = std::io::read_to_string(reader.by_name("word/numbering.xml").unwrap()).unwrap();

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
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();

    assert!(
        fn_xml.contains("m:oMath"),
        "footnotes.xml should contain m:oMath elements for math in footnotes"
    );
}

// ── Edge case: super/subscript preserved in headings ─────────────────

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

// ── Issue fixtures: competitor bug regression tests ─────────────────

fn fixture_doc_xml(fixture: &str) -> String {
    let path = format!("../../tests/fixtures/{fixture}.typ");
    let world = typort_core::TyportWorld::new(Path::new(&path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap()
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

/// Regression (figure rasterization + recovery.rs): a vector-drawing figure
/// (Bézier curve + an inner text label) must rasterize to a single embedded
/// image with its label baked into the pixels — not recovered into the body as
/// a stray paragraph (the label is absent from the HTML and would otherwise be
/// pulled from the paged output). A sibling table figure must stay an editable
/// table, and both captions must survive.
#[test]
fn drawing_figure_is_rasterized_not_leaked() {
    let xml = fixture_doc_xml("edge_figure_rasterized");
    assert_eq!(
        xml.matches("<w:drawing").count(),
        1,
        "the curve figure should be exactly one embedded image: {xml}"
    );
    assert_eq!(
        xml.matches("<w:tbl>").count(),
        1,
        "the table figure must stay an editable table, not be rasterized: {xml}"
    );
    assert!(
        !xml.contains("ZZLABELZZ"),
        "the canvas label must be baked into the image, not leaked as body text: {xml}"
    );
    assert!(
        xml.contains("Drawn figure") && xml.contains("Real table"),
        "both figure captions must survive: {xml}"
    );
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
    // dedup, not by hardcoded "图 "/"表 " keyword skipping (removed for P1).
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

// ── Edge-case fixtures: content assertions ──────────────────────────

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

// ── Round 3: competitor issue fixtures ──────────────────────────────

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
fn issue_cjk_latin_font_mixing_content() {
    let xml = fixture_doc_xml("issue_cjk_latin_font_mixing");
    assert!(xml.contains("中文正文"), "CJK text should be present");
    assert!(xml.contains("English"), "Latin text should be present");
    assert!(xml.contains("2024"), "numbers should be present");
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

// ── Round 4: competitor issue fixtures ──────────────────────────────

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

// ── Round 5: competitor issue fixtures ──────────────────────────────

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

// ── Round 6: competitor issue fixtures ──────────────────────────────

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

// ── Round 7: competitor issues ──────────────────────────────────────────

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
fn issue_cjk_font_east_asia() {
    let path = "../../tests/fixtures/issue_cjk_font_east_asia.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(archive.by_name("word/document.xml").unwrap()).unwrap();
    let styles_xml = std::io::read_to_string(archive.by_name("word/styles.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("eastAsia") || styles_xml.contains("eastAsia"),
        "should set w:rFonts eastAsia attribute for CJK (in document or styles)"
    );
    assert!(
        doc_xml.contains("\u{65E5}\u{672C}\u{8A9E}"),
        "Japanese text should be present"
    );
    assert!(
        doc_xml.contains("\u{4E2D}\u{6587}"),
        "Chinese text should be present"
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
fn issue_underline_font_size_change() {
    let xml = fixture_doc_xml("issue_underline_font_size_change");
    assert!(xml.contains("<w:u "), "should have underline formatting");
    assert!(
        xml.contains("w:strike"),
        "should have strikethrough formatting"
    );
    assert!(xml.contains("Normal size"), "underlined text present");
    assert!(xml.contains("Regular"), "strikethrough text present");
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

// ── Round 8: competitor issues ──────────────────────────────────────────

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

// ── Round 9: competitor issues ──────────────────────────────────────────

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

// ── Round 10: competitor issues (final round) ───────────────────────────

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

// ── Orphan fixture coverage ─────────────────────────────────────────────

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

// ── Language detection: w:lang derived from #set text(lang:), not guessed ──

/// Convert a fixture and return its `word/styles.xml` (where `w:lang` lives).
fn fixture_styles_xml(fixture: &str) -> String {
    let path = format!("../../tests/fixtures/{fixture}.typ");
    let world = typort_core::TyportWorld::new(Path::new(&path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    std::io::read_to_string(reader.by_name("word/styles.xml").unwrap()).unwrap()
}

#[test]
fn lang_german_is_de_de_not_guessed() {
    // A German document (no CJK) must derive de-DE from #set text(lang: "de"),
    // not fall back to the en-US/zh-CN guess. Guards against P1 regressions.
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
            let path = format!("../../tests/fixtures/{}.typ", $fixture);
            let world = typort_core::TyportWorld::new(std::path::Path::new(&path)).unwrap();
            let doc = typort_core::convert::convert(&world).unwrap();
            let mut buf = Vec::new();
            typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();
            assert!(buf.len() > 100, "docx output should be non-trivial");
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
smoke_test!(smoke_style_footnotes, "style_footnotes");
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

// ── Bibliography / Citation ────────────────────────────────────────────

#[test]
fn bibliography_citations_are_markers_not_field_refs() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Citations must NOT be REF fields to bibliography keys: a REF to a
    // non-existent bookmark renders in Word as "Error! Reference source not
    // found". They render instead as the marker Typst produced.
    for key in ["REF smith2020", "REF knuth1997", "REF wang2023"] {
        assert!(
            !xml.contains(key),
            "citation must not be a REF field: {key}"
        );
    }
    // The default style renders inline numeric markers ([1]/[2]/[3]).
    assert!(
        xml.contains("[1]") && xml.contains("[2]") && xml.contains("[3]"),
        "expected inline numeric citation markers: {xml}"
    );
}

#[test]
fn superscript_citation_style_raises_the_marker() {
    let xml = fixture_doc_xml("bibliography_superscript");
    // A superscript numeric style (here "nature") must raise the in-text marker,
    // detected from the rendered <sup> — not assumed from the style name.
    assert!(
        !xml.contains("REF smith2020"),
        "citation must not be a broken REF field: {xml}"
    );
    assert!(
        xml.contains(r#"<w:vertAlign w:val="superscript"/>"#),
        "superscript citation style must produce a raised marker: {xml}"
    );
}

#[test]
fn bibliography_entries_are_not_a_bulleted_list() {
    // Regression: bibliography entries arrive as a Typst <ul>, so each kept a
    // bullet-list numPr on top of its "[n]" label — a double marker. The "[n]" is
    // the marker; entries should carry only a hanging indent, no list numbering.
    let doc_xml = fixture_doc_xml("bibliography_basic");
    let entry = paragraph_containing(&doc_xml, "An Example Article");
    assert!(
        !entry.contains("<w:numPr>"),
        "bibliography entries must not be a bulleted/numbered list:\n{entry}"
    );
    assert!(
        entry.contains("w:hanging"),
        "bibliography entries should keep a hanging indent"
    );
}

#[test]
fn bibliography_style_is_defined() {
    // The reference field-code paragraph carries the "Bibliography" style; it must
    // be defined in styles.xml, not left as a dangling reference relying on Word's
    // built-in (which WPS/LibreOffice may not have).
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/bibliography_basic.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let styles = std::io::read_to_string(reader.by_name("word/styles.xml").unwrap()).unwrap();
    assert!(
        styles.contains(r#"w:styleId="Bibliography""#),
        "styles.xml should define the Bibliography style"
    );
}

#[test]
fn bibliography_produces_bibliography_sdt() {
    let xml = fixture_doc_xml("bibliography_basic");
    assert!(
        xml.contains("w:bibliography"),
        "expected w:bibliography SDT marker"
    );
    assert!(
        xml.contains("BIBLIOGRAPHY"),
        "expected BIBLIOGRAPHY field code"
    );
}

#[test]
fn bibliography_has_custom_xml_sources() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    assert!(
        archive.by_name("customXml/item1.xml").is_ok(),
        "expected customXml/item1.xml in ZIP"
    );
    let xml = std::io::read_to_string(archive.by_name("customXml/item1.xml").unwrap()).unwrap();
    assert!(xml.contains("b:Sources"), "expected b:Sources root");
    assert!(xml.contains("smith2020"), "expected smith2020 tag");
    assert!(xml.contains("knuth1997"), "expected knuth1997 tag");
    assert!(xml.contains("wang2023"), "expected wang2023 tag");
    assert!(
        xml.contains("JournalArticle"),
        "expected JournalArticle source type"
    );
    assert!(
        xml.contains("Book"),
        "expected Book source type for knuth1997"
    );
}

#[test]
fn bibliography_custom_xml_has_author_metadata() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let xml = std::io::read_to_string(archive.by_name("customXml/item1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("<b:Last>Smith</b:Last>"),
        "expected Smith author"
    );
    assert!(xml.contains("<b:Year>2020</b:Year>"), "expected year 2020");
    assert!(xml.contains("<b:Title>"), "expected title element");
    assert!(
        xml.contains("<b:Last>Knuth</b:Last>"),
        "expected Knuth author"
    );
    assert!(xml.contains("<b:Year>1997</b:Year>"), "expected year 1997");
}

#[test]
fn bibliography_citation_sources_count() {
    let path = "../../tests/fixtures/bibliography_basic.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    assert_eq!(
        doc.citation_sources.len(),
        3,
        "expected 3 citation sources, got {}",
        doc.citation_sources.len()
    );
}

#[test]
fn bibliography_field_codes_have_begin_and_end() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Both inline citations and bibliography block use field codes
    assert!(
        xml.contains("fldCharType=\"begin\""),
        "expected fldChar begin in citation field"
    );
    assert!(
        xml.contains("fldCharType=\"end\""),
        "expected fldChar end in citation field"
    );
}

#[test]
fn bibliography_display_text_present() {
    let xml = fixture_doc_xml("bibliography_basic");
    // The display text for citations should appear (e.g., "[1]", "[2]", "[3]")
    assert!(xml.contains("[1]"), "expected display text [1]");
    assert!(xml.contains("[2]"), "expected display text [2]");
    assert!(xml.contains("[3]"), "expected display text [3]");
}

#[test]
fn bibliography_section_heading_present() {
    let xml = fixture_doc_xml("bibliography_basic");
    // The bibliography section heading should be present
    assert!(
        xml.contains("Bibliography"),
        "expected Bibliography heading text"
    );
}

#[test]
fn bibliography_body_text_preserved() {
    let xml = fixture_doc_xml("bibliography_basic");
    // Body text around citations should be preserved
    assert!(
        xml.contains("Introduction"),
        "expected Introduction heading"
    );
    assert!(xml.contains("Methods"), "expected Methods heading");
    assert!(
        xml.contains("methodology is sound"),
        "expected body text near citation"
    );
}

#[test]
fn run_coalescing_collapses_split_line() {
    // Regression: the HTML walk emits one <w:r> per Typst text/space node, so a
    // plain line is shattered into many runs. The coalescing post-pass merges
    // adjacent equally-formatted runs while preserving the bold boundary.
    // See tests/fixtures/edge_run_coalescing.typ.
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
fn no_math_fallback_font_on_plain_digit_or_whitespace() {
    // Regression: per-glyph math fallback must not leak a math font onto plain
    // text, and a whitespace-only run must not carry a stray size. Typst shapes
    // the isolated digit '7' in "[7]" with a math-table face; copying the paged
    // run style verbatim used to emit `w:rFonts w:ascii="...Math"` on that digit.
    // Detection now normalizes any FontFlags::MATH face (and any non-letter run
    // whose font differs from baseline) back to the baseline, and drops all
    // overrides on whitespace-only runs. See tests/fixtures/edge_math_fallback_digit.typ.
    let xml = fixture_doc_xml("edge_math_fallback_digit");

    // (a) No run carries a *Math* face on its rFonts (the OpenType math family
    //     name marker), anywhere in the document.
    for rfonts in xml.split("<w:rFonts").skip(1) {
        let tag = rfonts.split('>').next().unwrap_or("");
        assert!(
            !tag.contains("Math"),
            "no run should carry a math fallback font; found rFonts: <w:rFonts{tag}>"
        );
    }

    // (b) The bare digit '7' must still survive as body text.
    assert!(
        xml.contains(">7<"),
        "expected the digit 7 to survive as a run"
    );

    // (c) A whitespace-only run must not carry a size override.
    for run in xml.split("<w:r>").skip(1) {
        let body = run.split("</w:r>").next().unwrap_or("");
        let Some(after_t) = body.split("<w:t").nth(1) else {
            continue;
        };
        let text = after_t
            .split_once('>')
            .and_then(|(_, rest)| rest.split("</w:t>").next())
            .unwrap_or("");
        if !text.is_empty() && text.chars().all(char::is_whitespace) {
            assert!(
                !body.contains("<w:sz "),
                "whitespace-only run must inherit size, got run: {body}"
            );
        }
    }
}

#[test]
fn fr_column_tracks_produce_proportional_widths() {
    // Regression: `columns: (1fr, 2fr, 3fr)` must yield a 1:2:3 width split, not
    // three equal columns. The writer falls back to equal distribution unless
    // cell.width_pct is populated from the Typst column track sizes.
    // See tests/fixtures/edge_table_fr_columns.typ.
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

#[test]
fn hanging_indent_par_set_rule_is_honored() {
    // `#set par(hanging-indent: 2em)` is a declared value recovered from the
    // source AST: paragraphs after it get a hanging indent, the one before stays
    // flush. No genre/keyword matching. See tests/fixtures/edge_hanging_indent_par.typ.
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

// ===========================================================================
// Golden snapshots: pin the exact `word/document.xml` for a curated fixture set
// so any output-formatting drift surfaces as a reviewable diff (the suite's only
// oracle for output *quality*, not mere presence). quick-xml already emits
// deterministic 2-space-indented XML (writer.rs `new_with_indent`), verified
// byte-identical across separate processes, so we snapshot it verbatim — no
// pretty-printer, no new dependency.
//
// CURATION — CI-safety: the World loads system fonts (world.rs
// `include_system_fonts(true)`), so a fixture whose CJK font is *detected* from
// rendering (not declared) pins a machine-specific font name (e.g. "KaiTi" on
// the dev box, something else on CI) and would flake. The set below is therefore
// limited to fixtures whose fonts are embedded (Libertinus), constant ("Courier
// New"), or DECLARED in source (complex_paper → "Noto Serif SC", read from the
// AST and thus environment-independent). CJK fixtures that rely on *detected*
// fonts (hello, issue_cjk_heading_numbering, edge_three_line_table) are
// deliberately excluded — they are covered by the substring-based tests above.
//
// Regenerate after an intentional change, then review the diff before committing:
//   UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden
//   git diff tests/snapshots
mod golden {
    use super::fixture_doc_xml;

    /// Path of a committed golden, relative to the crate dir (where tests run),
    /// mirroring the `../../tests/...` convention `fixture_doc_xml` uses.
    fn golden_path(fixture: &str) -> std::path::PathBuf {
        std::path::Path::new("../../tests/snapshots").join(format!("{fixture}.document.xml"))
    }

    /// Normalize for comparison: strip trailing whitespace per line and force LF,
    /// defending against CRLF checkouts and editor end-of-line churn.
    fn normalize_xml(xml: &str) -> String {
        let body = xml.replace("\r\n", "\n");
        let mut out: String = body
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out
    }

    /// First differing line, for a one-line failure message instead of a huge
    /// dump. Returns `(line_no, expected, actual)`.
    fn first_diff<'a>(expected: &'a str, actual: &'a str) -> Option<(usize, &'a str, &'a str)> {
        for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
            if e != a {
                return Some((i + 1, e, a));
            }
        }
        let (el, al) = (expected.lines().count(), actual.lines().count());
        if el != al {
            return Some((
                el.min(al) + 1,
                "<line count differs>",
                "<line count differs>",
            ));
        }
        None
    }

    /// Convert the fixture, normalize, and either (re)write the golden
    /// (`UPDATE_SNAPSHOTS=1`) or assert byte-equality against the committed one.
    fn check_golden(fixture: &str) {
        let actual = normalize_xml(&fixture_doc_xml(fixture));
        let path = golden_path(fixture);

        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &actual)
                .unwrap_or_else(|e| panic!("failed to write golden {}: {e}", path.display()));
            return;
        }

        let expected = match std::fs::read_to_string(&path) {
            Ok(s) => normalize_xml(&s),
            Err(_) => panic!(
                "missing golden {}\nregenerate with: \
                 UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden",
                path.display()
            ),
        };

        if let Some((line, e, a)) = first_diff(&expected, &actual) {
            panic!(
                "golden mismatch for {fixture} at line {line}\n  expected: {e}\n  actual:   {a}\n\
                 \nif this change is intentional, regenerate and review:\n  \
                 UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden\n  \
                 git diff tests/snapshots"
            );
        }
    }

    macro_rules! golden_test {
        ($name:ident, $fixture:literal) => {
            #[test]
            fn $name() {
                check_golden($fixture);
            }
        };
    }

    golden_test!(golden_complex_paper, "complex_paper");
    golden_test!(golden_aligned_equations, "aligned_equations");
    golden_test!(golden_inline_math_in_text, "inline_math_in_text");
    golden_test!(golden_edge_complex_table, "edge_complex_table");
    golden_test!(golden_formatted_footnote, "formatted_footnote");
    golden_test!(golden_edge_term_list, "edge_term_list");
    golden_test!(golden_edge_deep_nested_list, "edge_deep_nested_list");
    golden_test!(golden_bibliography_basic, "bibliography_basic");
    golden_test!(golden_edge_theorem_proof, "edge_theorem_proof");
}

#[test]
fn deliberate_digit_run_font_override_is_kept() {
    // A digits-only run the author explicitly set in a non-body font must keep
    // its w:rFonts — only a true OpenType MATH-table fallback is normalized away.
    // See tests/fixtures/edge_digit_run_font.typ.
    let xml = fixture_doc_xml("edge_digit_run_font");
    // The run "12345" must survive carrying its declared monospace face.
    let para = paragraph_containing(&xml, "12345");
    assert!(
        para.contains("DejaVu Sans Mono"),
        "deliberate per-run font on a digit run must be preserved:\n{para}"
    );
}

#[test]
fn hanging_indent_does_not_clobber_list_items() {
    // A `#set par(hanging-indent: 2em)` rule must not override a list item's own
    // indent. List items keep the list hanging indent (left 2em / hanging 1em =
    // 440/220 at the 11pt default), never the bibliography 2em/2em (440/440).
    // See tests/fixtures/edge_hanging_indent_list.typ.
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

#[test]
fn footnote_and_table_not_recovered_as_body_orphans() {
    // The footnote body must live in the footnote zone, not be scraped into the
    // document body; and no horizontal rule may be invented from the footnote
    // separator or the table's border lines (the source declares no #line()).
    // See tests/fixtures/edge_footnote_table_no_orphan.typ.
    let doc_xml = fixture_doc_xml("edge_footnote_table_no_orphan");
    assert!(
        !doc_xml.contains("must stay in the footnote zone"),
        "footnote body must not be duplicated into the document body:\n{doc_xml}"
    );
    assert!(
        !doc_xml.contains("<w:pBdr>"),
        "no horizontal rule should be invented without a source #line():\n{doc_xml}"
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
fn wrapped_table_row_not_recovered_as_orphan() {
    // A table cell that wraps in a narrow column must not be re-scraped into the
    // body as a tab-stop orphan row (the wrap truncates the cell so text-dedup
    // misses it). "主成分分析法" lives only in complex_paper's measurement table.
    let doc_xml = fixture_doc_xml("complex_paper");
    assert_eq!(
        doc_xml.matches("主成分分析法").count(),
        1,
        "table cell text must appear once (in the table), not duplicated as a recovered orphan"
    );
}
