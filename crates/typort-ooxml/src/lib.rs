#![warn(clippy::pedantic)]

pub mod document;
pub mod styles;
pub mod writer;

pub use document::{
    CitationSource, Document, DocumentMetadata, DocumentStyle, FootnoteFormat, HeaderFooter,
    ImageData, ImageFormat, PageNumberFormat, PersonName, SectionBreak, SectionBreakType,
    SourceType,
};
pub use writer::write_docx;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Helper: build a docx from a Document and return the ZIP archive bytes.
    fn build_docx(doc: &Document) -> Vec<u8> {
        let mut buf = Vec::new();
        write_docx(doc, Cursor::new(&mut buf)).unwrap();
        buf
    }

    /// Helper: extract a named file from a docx ZIP archive as a String.
    fn read_zip_entry(buf: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(buf)).unwrap();
        std::io::read_to_string(archive.by_name(name).unwrap()).unwrap()
    }

    /// Helper: check whether a named file exists in the ZIP archive.
    fn zip_has_entry(buf: &[u8], name: &str) -> bool {
        let archive = zip::ZipArchive::new(Cursor::new(buf)).unwrap();
        archive.file_names().any(|n| n == name)
    }

    // ── Existing tests ──────────────────────────────────────────────────

    #[test]
    fn minimal_docx_is_valid_zip_with_content_types() {
        let doc = Document::new();
        let mut buf = Vec::new();
        write_docx(&doc, Cursor::new(&mut buf)).unwrap();

        let reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        let names: Vec<&str> = reader.file_names().collect();
        assert!(names.contains(&"[Content_Types].xml"));
        assert!(names.contains(&"word/document.xml"));
        assert!(names.contains(&"_rels/.rels"));
        assert!(names.contains(&"word/_rels/document.xml.rels"));
    }

    #[test]
    fn docx_contains_paragraph_text() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("你好世界");
        doc.add_paragraph(para);

        let mut buf = Vec::new();
        write_docx(&doc, Cursor::new(&mut buf)).unwrap();

        let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        let doc_xml =
            std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
        assert!(doc_xml.contains("你好世界"));
    }

    // ── 1. Run formatting ───────────────────────────────────────────────

    #[test]
    fn run_bold_produces_w_b() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("bold text");
        run.bold = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("<w:b/>"), "expected <w:b/> in: {xml}");
        assert!(xml.contains("bold text"));
    }

    #[test]
    fn run_italic_produces_w_i() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("italic text");
        run.italic = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("<w:i/>"), "expected <w:i/> in: {xml}");
    }

    #[test]
    fn run_superscript_produces_vert_align_superscript() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("sup");
        run.superscript = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:vertAlign w:val="superscript"/>"#),
            "expected vertAlign superscript in: {xml}"
        );
    }

    #[test]
    fn run_subscript_produces_vert_align_subscript() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("sub");
        run.subscript = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:vertAlign w:val="subscript"/>"#),
            "expected vertAlign subscript in: {xml}"
        );
    }

    #[test]
    fn run_monospace_produces_courier_new_font() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("code");
        run.monospace = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:ascii="Courier New"#),
            "expected Courier New font in: {xml}"
        );
    }

    // ── 2. Paragraph styles ─────────────────────────────────────────────

    #[test]
    fn heading_1_produces_pstyle_heading1() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.style = Some(document::ParagraphStyle::Heading(1));
        para.add_run("Title");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:pStyle w:val="Heading1"/>"#),
            "expected pStyle Heading1 in: {xml}"
        );
    }

    #[test]
    fn normal_style_produces_pstyle_normal() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.style = Some(document::ParagraphStyle::Normal);
        para.add_run("Body text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:pStyle w:val="Normal"/>"#),
            "expected pStyle Normal in: {xml}"
        );
    }

    // ── 3. Alignment ────────────────────────────────────────────────────

    #[test]
    fn alignment_center_produces_jc_center() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.alignment = Some(document::Alignment::Center);
        para.add_run("centered");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:jc w:val="center"/>"#),
            "expected jc center in: {xml}"
        );
    }

    #[test]
    fn alignment_right_produces_jc_right() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.alignment = Some(document::Alignment::Right);
        para.add_run("right");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:jc w:val="right"/>"#),
            "expected jc right in: {xml}"
        );
    }

    #[test]
    fn alignment_justify_produces_jc_both() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.alignment = Some(document::Alignment::Justify);
        para.add_run("justified");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:jc w:val="both"/>"#),
            "expected jc both in: {xml}"
        );
    }

    // ── 4. Indent suppression ───────────────────────────────────────────

    #[test]
    fn suppress_indent_produces_first_line_zero() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.suppress_indent = true;
        para.add_run("no indent");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:firstLine="0""#),
            "expected firstLine=0 in: {xml}"
        );
    }

    // ── 5. Hanging indent ───────────────────────────────────────────────

    #[test]
    fn hanging_indent_produces_hanging_and_left() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.hanging_indent = true;
        para.add_run("bibliography entry");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:hanging="440""#),
            "expected hanging=440 in: {xml}"
        );
        assert!(
            xml.contains(r#"w:left="440""#),
            "expected left=440 in: {xml}"
        );
    }

    // ── 6. Left indent ──────────────────────────────────────────────────

    #[test]
    fn left_indent_produces_w_left() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.left_indent = Some(720);
        para.add_run("indented block");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:left="720""#),
            "expected left=720 in: {xml}"
        );
    }

    // ── 7. Code block ───────────────────────────────────────────────────

    #[test]
    fn code_block_produces_pstyle_codeblock() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.code_block = true;
        para.add_run("fn main() {}");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:pStyle w:val="CodeBlock"/>"#),
            "expected pStyle CodeBlock in: {xml}"
        );
    }

    // ── 8. List numbering ───────────────────────────────────────────────

    #[test]
    fn list_item_produces_num_pr_with_ilvl_and_num_id() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.list_info = Some(document::ListInfo { id: 1, level: 0 });
        para.add_run("list item");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("<w:numPr>"), "expected w:numPr in: {xml}");
        assert!(
            xml.contains(r#"<w:ilvl w:val="0"/>"#),
            "expected ilvl 0 in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:numId w:val="1"/>"#),
            "expected numId 1 in: {xml}"
        );
    }

    // ── 9. Footnote reference (decimal) ─────────────────────────────────

    #[test]
    fn footnote_ref_decimal_emits_footnote_reference() {
        let mut doc = Document::new();
        let fn_id = doc.add_footnote(vec![document::InlineElement::Text(document::Run::new("note text"))]);
        let mut para = document::Paragraph::new();
        para.add_run("See");
        para.add_footnote_ref(fn_id);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        let expected = format!(r#"<w:footnoteReference w:id="{fn_id}"/>"#);
        assert!(xml.contains(&expected), "expected {expected} in: {xml}");
        // Decimal mode should NOT have customMarkFollows
        assert!(
            !xml.contains("customMarkFollows"),
            "decimal mode should not have customMarkFollows"
        );
    }

    // ── 10. Footnote reference (circled) ────────────────────────────────

    #[test]
    fn footnote_ref_circled_emits_custom_mark_follows() {
        let mut doc = Document::new();
        doc.style.footnote_format = FootnoteFormat::CircledNumber;
        let fn_id = doc.add_footnote(vec![document::InlineElement::Text(document::Run::new("circled note"))]);
        let mut para = document::Paragraph::new();
        para.add_footnote_ref(fn_id);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:customMarkFollows="1""#),
            "expected customMarkFollows in: {xml}"
        );
        // fn_id=2, seq_num=1 => circled char U+2460 = \u{2460}
        assert!(
            xml.contains('\u{2460}'),
            "expected circled digit 1 (\u{2460}) in: {xml}"
        );
    }

    // ── 11. Footnote content ────────────────────────────────────────────

    #[test]
    fn footnotes_xml_contains_footnote_text_and_ref() {
        let mut doc = Document::new();
        doc.add_footnote(vec![document::InlineElement::Text(document::Run::new("This is a footnote."))]);
        // Need at least one paragraph referencing it so the doc is valid,
        // but footnotes.xml is generated from doc.footnotes regardless.
        let mut para = document::Paragraph::new();
        para.add_footnote_ref(2);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/footnotes.xml");
        assert!(
            xml.contains("This is a footnote."),
            "expected footnote text in: {xml}"
        );
        assert!(
            xml.contains("<w:footnoteRef/>"),
            "expected w:footnoteRef in footnotes.xml: {xml}"
        );
        assert!(
            xml.contains(r#"<w:pStyle w:val="FootnoteText"/>"#),
            "expected FootnoteText style in: {xml}"
        );
    }

    // ── 12. Table borders ───────────────────────────────────────────────

    #[test]
    fn table_produces_tbl_borders() {
        let mut doc = Document::new();
        let cell = document::TableCell {
            paragraphs: vec![],
            content: Vec::new(),
            colspan: 1,
            vmerge: document::VMerge::None,
            width_pct: None,
        };
        let row = document::TableRow { cells: vec![cell] };
        let table = document::Table {
            rows: vec![row],
            width_pct: None,
            border_size: None,
        };
        doc.add_table(table);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("<w:tblBorders>"),
            "expected w:tblBorders in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:top w:val="single""#),
            "expected top border in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:bottom w:val="single""#),
            "expected bottom border in: {xml}"
        );
    }

    // ── 13. Settings.xml ────────────────────────────────────────────────

    #[test]
    fn settings_xml_contains_footnote_pr_compat_theme_font() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/settings.xml");
        assert!(
            xml.contains("<w:footnotePr>"),
            "expected w:footnotePr in settings.xml: {xml}"
        );
        assert!(
            xml.contains("<w:compat>"),
            "expected w:compat in settings.xml: {xml}"
        );
        assert!(
            xml.contains("<w:themeFontLang"),
            "expected w:themeFontLang in settings.xml: {xml}"
        );
        assert!(
            xml.contains(r#"w:eastAsia="zh-CN""#),
            "expected zh-CN in themeFontLang: {xml}"
        );
    }

    // ── 14. Styles.xml ──────────────────────────────────────────────────

    #[test]
    fn styles_xml_normal_has_jc_left_and_first_line_indent() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        // Normal style uses left alignment (Typst default is justify: false)
        assert!(
            xml.contains(r#"<w:jc w:val="left"/>"#),
            "expected jc left in Normal style: {xml}"
        );
        // Typst default first-line indent is 0 (no indent)
        assert!(
            xml.contains(r#"w:firstLine="0""#),
            "expected firstLine=0 in Normal style: {xml}"
        );
    }

    #[test]
    fn styles_xml_heading_has_bold_and_outline_level() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        // Heading styles should have w:b for bold
        assert!(
            xml.contains(r#"w:styleId="Heading1""#),
            "expected Heading1 style definition: {xml}"
        );
        assert!(
            xml.contains("<w:b/>"),
            "expected bold in heading style: {xml}"
        );
        // Outline level 0 for Heading1
        assert!(
            xml.contains(r#"<w:outlineLvl w:val="0"/>"#),
            "expected outlineLvl 0 for Heading1: {xml}"
        );
    }

    #[test]
    fn styles_xml_has_footnote_styles_when_footnotes_present() {
        let mut doc = Document::new();
        doc.add_footnote(vec![document::InlineElement::Text(document::Run::new("fn"))]);
        let mut para = document::Paragraph::new();
        para.add_footnote_ref(2);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        assert!(
            xml.contains(r#"w:styleId="FootnoteReference""#),
            "expected FootnoteReference style: {xml}"
        );
        assert!(
            xml.contains(r#"w:styleId="FootnoteText""#),
            "expected FootnoteText style: {xml}"
        );
    }

    // ── 15. Empty document ──────────────────────────────────────────────

    #[test]
    fn empty_document_no_crash_valid_zip() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let archive = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        assert!(archive.len() > 0, "ZIP should have at least one entry");
    }

    // ── 16. Document without footnotes ──────────────────────────────────

    #[test]
    fn document_without_footnotes_has_no_footnotes_xml() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("no footnotes here");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(
            !zip_has_entry(&buf, "word/footnotes.xml"),
            "expected no footnotes.xml when there are no footnotes"
        );
    }

    // ── 17. Document without lists ──────────────────────────────────────

    #[test]
    fn document_without_lists_has_no_numbering_xml() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("no lists here");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(
            !zip_has_entry(&buf, "word/numbering.xml"),
            "expected no numbering.xml when there are no lists"
        );
    }

    // ── 18. Math inline element ─────────────────────────────────────────

    #[test]
    fn math_inline_passes_through_omml_xml() {
        let omml = r#"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#;
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_math(omml.to_string());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains(omml), "expected OMML passthrough in: {xml}");
    }

    // ── 19. Document grid ────────────────────────────────────────────────

    #[test]
    fn doc_grid_in_section_properties() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("w:docGrid"),
            "should have docGrid in sectPr: {xml}"
        );
        assert!(
            xml.contains("w:linePitch"),
            "should have linePitch attribute: {xml}"
        );
        assert!(
            !xml.contains(r#"w:type="lines""#),
            "docGrid should NOT use type=lines (it adds pitch on top of paragraph spacing): {xml}"
        );
    }

    // ── 20. CJK properties in docDefaults ──────────────────────────────

    #[test]
    fn cjk_properties_in_doc_defaults() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        assert!(
            xml.contains("<w:kinsoku/>"),
            "should have kinsoku in docDefaults: {xml}"
        );
        assert!(
            xml.contains("<w:overflowPunct/>"),
            "should have overflowPunct in docDefaults: {xml}"
        );
        assert!(
            xml.contains("<w:autoSpaceDE/>"),
            "should have autoSpaceDE in docDefaults: {xml}"
        );
        assert!(
            xml.contains("<w:autoSpaceDN/>"),
            "should have autoSpaceDN in docDefaults: {xml}"
        );
        assert!(
            xml.contains("<w:wordWrap/>"),
            "should have wordWrap in docDefaults: {xml}"
        );
        assert!(
            xml.contains("<w:topLinePunct/>"),
            "should have topLinePunct in docDefaults: {xml}"
        );
    }

    // ── 21. Heading paragraph control properties ───────────────────────

    #[test]
    fn heading_styles_have_keep_next_and_widow_control() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        assert!(
            xml.contains("<w:keepNext/>"),
            "heading styles should have keepNext: {xml}"
        );
        assert!(
            xml.contains("<w:widowControl/>"),
            "should have widowControl: {xml}"
        );
    }

    // ── 22. Numbered equation ──────────────────────────────────────────

    #[test]
    fn numbered_equation_produces_right_tab_and_number_text() {
        let omml = r#"<m:oMath><m:r><m:t>E=mc^2</m:t></m:r></m:oMath>"#;
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_numbered_math(omml.to_string(), "(1)".to_string());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        // Right tab stop at 8306
        assert!(
            xml.contains(r#"<w:tab w:val="right" w:pos="8306"/>"#),
            "expected right tab stop in: {xml}"
        );
        // Tab character in a run
        assert!(xml.contains("<w:tab/>"), "expected tab element in: {xml}");
        // Equation number text
        assert!(
            xml.contains("(1)"),
            "expected equation number (1) in: {xml}"
        );
        // Suppress first-line indent for equation paragraphs
        assert!(
            xml.contains(r#"w:firstLine="0""#),
            "expected firstLine=0 for equation paragraph: {xml}"
        );
    }

    // ── 23. Image embedding ───────────────────────────────────────────────

    #[test]
    fn image_inline_produces_drawing_xml() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_image(document::ImageData {
            bytes: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes
            format: document::ImageFormat::Png,
            width_emu: 914_400,  // 1 inch
            height_emu: 457_200, // 0.5 inch
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("w:drawing"), "expected w:drawing in: {xml}");
        assert!(xml.contains("wp:inline"), "expected wp:inline in: {xml}");
        assert!(xml.contains("a:blip"), "expected a:blip in: {xml}");
        assert!(
            xml.contains(r#"cx="914400""#),
            "expected cx=914400 in: {xml}"
        );
        assert!(
            xml.contains(r#"cy="457200""#),
            "expected cy=457200 in: {xml}"
        );
    }

    #[test]
    fn image_writes_to_zip_media() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header
        para.add_image(document::ImageData {
            bytes: png_data.clone(),
            format: document::ImageFormat::Png,
            width_emu: 914_400,
            height_emu: 914_400,
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(
            zip_has_entry(&buf, "word/media/image1.png"),
            "ZIP should contain word/media/image1.png"
        );

        // Verify the image bytes are correct
        let mut archive = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        let mut img_entry = archive.by_name("word/media/image1.png").unwrap();
        let mut img_bytes = Vec::new();
        std::io::Read::read_to_end(&mut img_entry, &mut img_bytes).unwrap();
        assert_eq!(img_bytes, png_data, "image bytes in ZIP should match input");
    }

    #[test]
    fn image_content_types_include_png() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_image(document::ImageData {
            bytes: vec![0x89],
            format: document::ImageFormat::Png,
            width_emu: 100,
            height_emu: 100,
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let ct = read_zip_entry(&buf, "[Content_Types].xml");
        assert!(
            ct.contains("image/png"),
            "content types should include image/png: {ct}"
        );
    }

    #[test]
    fn image_content_types_include_jpeg() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_image(document::ImageData {
            bytes: vec![0xFF, 0xD8],
            format: document::ImageFormat::Jpeg,
            width_emu: 100,
            height_emu: 100,
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let ct = read_zip_entry(&buf, "[Content_Types].xml");
        assert!(
            ct.contains("image/jpeg"),
            "content types should include image/jpeg: {ct}"
        );
        assert!(
            zip_has_entry(&buf, "word/media/image1.jpg"),
            "ZIP should contain word/media/image1.jpg"
        );
    }

    #[test]
    fn image_rels_include_image_relationship() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_image(document::ImageData {
            bytes: vec![0x89],
            format: document::ImageFormat::Png,
            width_emu: 100,
            height_emu: 100,
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let rels = read_zip_entry(&buf, "word/_rels/document.xml.rels");
        assert!(
            rels.contains("relationships/image"),
            "rels should include image relationship: {rels}"
        );
        assert!(
            rels.contains("media/image1.png"),
            "rels should reference media/image1.png: {rels}"
        );
    }

    #[test]
    fn document_xml_has_image_namespaces() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_image(document::ImageData {
            bytes: vec![0x89],
            format: document::ImageFormat::Png,
            width_emu: 100,
            height_emu: 100,
        });
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("xmlns:wp="),
            "document should have wp namespace: {xml}"
        );
        assert!(
            xml.contains("xmlns:a="),
            "document should have a namespace: {xml}"
        );
        assert!(
            xml.contains("xmlns:pic="),
            "document should have pic namespace: {xml}"
        );
    }

    #[test]
    fn document_without_images_has_no_image_namespaces() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("no images here");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            !xml.contains("xmlns:wp="),
            "document without images should not have wp namespace"
        );
    }

    // ── 25. Bookmark start/end ──────────────────────────────────────────

    #[test]
    fn bookmark_produces_bookmark_start_and_end() {
        let mut doc = Document::new();
        let bk_id = doc.next_bookmark_id();
        let mut para = document::Paragraph::new();
        para.add_bookmark(bk_id, "fig-demo".to_string());
        para.add_run("Figure 1: Demo");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:bookmarkStart w:id="0" w:name="fig-demo"/>"#),
            "expected bookmarkStart in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:bookmarkEnd w:id="0"/>"#),
            "expected bookmarkEnd in: {xml}"
        );
    }

    // ── 26. Cross-reference field (REF) ─────────────────────────────────

    #[test]
    fn field_ref_produces_fld_char_and_instr_text() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_field_ref("fig-demo".to_string(), "Figure 1".to_string());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"w:fldCharType="begin"#),
            "expected fldChar begin in: {xml}"
        );
        assert!(
            xml.contains("REF fig-demo"),
            "expected REF instrText in: {xml}"
        );
        assert!(
            xml.contains(r#"w:fldCharType="separate"#),
            "expected fldChar separate in: {xml}"
        );
        assert!(xml.contains("Figure 1"), "expected display text in: {xml}");
        assert!(
            xml.contains(r#"w:fldCharType="end"#),
            "expected fldChar end in: {xml}"
        );
    }

    // ── 27. Hyperlink ───────────────────────────────────────────────────

    #[test]
    fn hyperlink_produces_fld_simple_with_url() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_hyperlink(
            "https://example.com".to_string(),
            vec![document::Run::new("click here")],
        );
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("w:fldSimple"),
            "expected w:fldSimple in: {xml}"
        );
        assert!(
            xml.contains("HYPERLINK"),
            "expected HYPERLINK in instrText: {xml}"
        );
        assert!(
            xml.contains("https://example.com"),
            "expected URL in: {xml}"
        );
        assert!(
            xml.contains("click here"),
            "expected display text in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:color w:val="0563C1"/>"#),
            "expected blue color in: {xml}"
        );
        assert!(
            xml.contains(r#"<w:u w:val="single"/>"#),
            "expected underline in: {xml}"
        );
    }

    // ── 28. Page break ──────────────────────────────────────────────────

    #[test]
    fn page_break_produces_br_type_page() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_page_break();
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:br w:type="page"/>"#),
            "expected page break in: {xml}"
        );
    }

    // ── 29. Bookmark counter increments ─────────────────────────────────

    #[test]
    fn bookmark_counter_increments() {
        let mut doc = Document::new();
        let id1 = doc.next_bookmark_id();
        let id2 = doc.next_bookmark_id();
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
    }

    // ── 30. Run underline ─────────────────────────────────────────────

    #[test]
    fn run_underline_produces_w_u_single() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("underlined");
        run.underline = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:u w:val="single"/>"#),
            "expected <w:u w:val=\"single\"/> in: {xml}"
        );
        assert!(xml.contains("underlined"));
    }

    // ── 31. Run strikethrough ──────────────────────────────────────────

    #[test]
    fn run_strikethrough_produces_w_strike() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("deleted");
        run.strikethrough = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("<w:strike/>"),
            "expected <w:strike/> in: {xml}"
        );
        assert!(xml.contains("deleted"));
    }

    // ── 32. Run highlight ──────────────────────────────────────────────

    #[test]
    fn run_highlight_produces_w_highlight_yellow() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("highlighted");
        run.highlight_color = Some("yellow".into());
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r#"<w:highlight w:val="yellow"/>"#),
            "expected <w:highlight w:val=\"yellow\"/> in: {xml}"
        );
        assert!(xml.contains("highlighted"));
    }

    // ── 33. Run smallcaps ──────────────────────────────────────────────

    #[test]
    fn run_smallcaps_produces_w_smallcaps() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        let mut run = document::Run::new("Small Caps");
        run.smallcaps = true;
        para.push_run(run);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("<w:smallCaps/>"),
            "expected <w:smallCaps/> in: {xml}"
        );
        assert!(xml.contains("Small Caps"));
    }

    // ── 34. Horizontal rule ────────────────────────────────────────────

    #[test]
    fn horizontal_rule_produces_pbdr_bottom() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.horizontal_rule = true;
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("<w:pBdr>"), "expected w:pBdr in: {xml}");
        assert!(
            xml.contains(r#"<w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/>"#),
            "expected bottom border attributes in: {xml}"
        );
    }

    // ── 35. Combined cross-reference round-trip ─────────────────────────

    #[test]
    fn bookmark_and_ref_round_trip() {
        let mut doc = Document::new();

        // Target paragraph with bookmark
        let bk_id = doc.next_bookmark_id();
        let mut target_para = document::Paragraph::new();
        target_para.style = Some(document::ParagraphStyle::Heading(1));
        target_para.add_bookmark(bk_id, "intro".to_string());
        target_para.add_run("Introduction");
        doc.add_paragraph(target_para);

        // Paragraph with cross-reference
        let mut ref_para = document::Paragraph::new();
        ref_para.add_run("See ");
        ref_para.add_field_ref("intro".to_string(), "Introduction".to_string());
        doc.add_paragraph(ref_para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");

        // Both bookmark and REF field should be present
        assert!(xml.contains(r#"w:name="intro"#), "bookmark should exist");
        assert!(
            xml.contains("REF intro"),
            "REF field should reference bookmark"
        );
    }

    // ── 36. Page numbering — decimal ───────────────────────────────────

    #[test]
    fn page_numbering_decimal_generates_footer_with_page_field() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::Decimal);
        let mut para = document::Paragraph::new();
        para.add_run("body text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        // Footer file should exist
        assert!(
            zip_has_entry(&buf, "word/footer1.xml"),
            "ZIP should contain word/footer1.xml when page_numbering is set"
        );
        let footer = read_zip_entry(&buf, "word/footer1.xml");
        // Should contain PAGE field code
        assert!(
            footer.contains(" PAGE "),
            "footer should contain PAGE instrText: {footer}"
        );
        assert!(
            footer.contains(r#"w:fldCharType="begin"#),
            "footer should contain fldChar begin: {footer}"
        );
        assert!(
            footer.contains(r#"w:fldCharType="separate"#),
            "footer should contain fldChar separate: {footer}"
        );
        assert!(
            footer.contains(r#"w:fldCharType="end"#),
            "footer should contain fldChar end: {footer}"
        );
        // Should be center-aligned
        assert!(
            footer.contains(r#"<w:jc w:val="center"/>"#),
            "footer PAGE field should be centered: {footer}"
        );
    }

    // ── 37. Page numbering — footer NOT static text ────────────────────

    #[test]
    fn page_numbering_footer_does_not_contain_static_page_number_body() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::Decimal);
        let mut para = document::Paragraph::new();
        para.add_run("body text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let doc_xml = read_zip_entry(&buf, "word/document.xml");
        // The body should NOT contain the page number as a static run
        // (it's in the footer via PAGE field, not in the body)
        assert!(
            !doc_xml.contains(r#"<w:t xml:space="preserve">1</w:t>"#) || doc_xml.contains("PAGE"),
            "body should not contain static page number text"
        );
    }

    // ── 38. Page numbering — sectPr references footer ──────────────────

    #[test]
    fn page_numbering_sect_pr_references_footer() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::Decimal);
        let mut para = document::Paragraph::new();
        para.add_run("text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let doc_xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            doc_xml.contains("w:footerReference"),
            "sectPr should reference footer1.xml: {doc_xml}"
        );
    }

    // ── 39. Page numbering — pgNumType with decimal ────────────────────

    #[test]
    fn page_numbering_decimal_emits_pg_num_type() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::Decimal);
        let mut para = document::Paragraph::new();
        para.add_run("text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let doc_xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            doc_xml.contains(r#"<w:pgNumType w:fmt="decimal"/>"#),
            "sectPr should contain pgNumType decimal: {doc_xml}"
        );
    }

    // ── 40. Page numbering — lower roman ───────────────────────────────

    #[test]
    fn page_numbering_lower_roman_emits_pg_num_type() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::LowerRoman);
        let mut para = document::Paragraph::new();
        para.add_run("text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let doc_xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            doc_xml.contains(r#"<w:pgNumType w:fmt="lowerRoman"/>"#),
            "sectPr should contain pgNumType lowerRoman: {doc_xml}"
        );
    }

    // ── 41. Page numbering — content types and rels ────────────────────

    #[test]
    fn page_numbering_generates_content_types_and_rels() {
        let mut doc = Document::new();
        doc.page_numbering = Some(PageNumberFormat::Decimal);
        let mut para = document::Paragraph::new();
        para.add_run("text");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let ct = read_zip_entry(&buf, "[Content_Types].xml");
        assert!(
            ct.contains("footer"),
            "content types should reference footer: {ct}"
        );
        let rels = read_zip_entry(&buf, "word/_rels/document.xml.rels");
        assert!(
            rels.contains("footer"),
            "document rels should reference footer: {rels}"
        );
    }

    // ── 42. No page numbering — no footer generated ────────────────────

    #[test]
    fn no_page_numbering_no_footer() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("no page numbers");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(
            !zip_has_entry(&buf, "word/footer1.xml"),
            "ZIP should NOT contain footer1.xml when no page numbering and no footer"
        );
    }

    // ── 43. Citation source on document ────────────────────────────────

    #[test]
    fn citation_source_on_document() {
        use document::{CitationSource, PersonName, SourceType};
        let mut doc = Document::new();
        doc.citation_sources.push(CitationSource {
            tag: "Smi20".into(),
            source_type: SourceType::JournalArticle,
            authors: vec![PersonName {
                last: "Smith".into(),
                first: Some("John".into()),
                middle: Some("A.".into()),
            }],
            title: Some("Example Article".into()),
            year: Some("2020".into()),
            journal_name: Some("Journal of Examples".into()),
            volume: Some("42".into()),
            issue: Some("3".into()),
            pages: Some("100-115".into()),
            doi: Some("10.1234/example".into()),
            url: None,
            publisher: None,
            city: None,
            edition: None,
            book_title: None,
        });
        assert_eq!(doc.citation_sources.len(), 1);
        assert_eq!(doc.citation_sources[0].source_type.as_ooxml_str(), "JournalArticle");
    }

    // ── 44. Paragraph add citation ─────────────────────────────────────

    #[test]
    fn paragraph_add_citation() {
        let mut para = document::Paragraph::new();
        para.add_citation(vec!["Smi20".into()], "(Smith, 2020)".into());
        assert_eq!(para.inlines.len(), 1);
        assert!(matches!(
            &para.inlines[0],
            document::InlineElement::Citation { keys, display_text }
                if keys == &["Smi20"] && display_text == "(Smith, 2020)"
        ));
    }

    // ── 45. Citation SDT with field code ──────────────────────────────

    #[test]
    fn citation_produces_sdt_with_field_code() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("See ");
        para.add_citation(vec!["Smi20".into()], "(Smith, 2020)".into());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("w:sdt"), "expected SDT wrapper in: {xml}");
        assert!(xml.contains("w:citation"), "expected w:citation marker in: {xml}");
        assert!(xml.contains("CITATION Smi20"), "expected CITATION field code in: {xml}");
        assert!(xml.contains("(Smith, 2020)"), "expected display text in: {xml}");
    }

    #[test]
    fn merged_citation_uses_backslash_m() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_citation(
            vec!["Smi20".into(), "Jon21".into()],
            "(Smith, 2020; Jones, 2021)".into(),
        );
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains(r"CITATION Smi20 \l 1033 \m Jon21"),
            "expected merged citation field in: {xml}"
        );
    }

    // ── 47. Custom XML bibliography data source ───────────────────────

    #[test]
    fn custom_xml_sources_contains_bibliography_entry() {
        use document::{CitationSource, PersonName, SourceType};
        let mut doc = Document::new();
        doc.citation_sources.push(CitationSource {
            tag: "Smi20".into(),
            source_type: SourceType::JournalArticle,
            authors: vec![PersonName {
                last: "Smith".into(),
                first: Some("John".into()),
                middle: Some("A.".into()),
            }],
            title: Some("Example Article".into()),
            year: Some("2020".into()),
            journal_name: Some("Journal of Examples".into()),
            volume: Some("42".into()),
            issue: Some("3".into()),
            pages: Some("100-115".into()),
            doi: Some("10.1234/example".into()),
            url: None,
            publisher: None,
            city: None,
            edition: None,
            book_title: None,
        });
        let mut para = document::Paragraph::new();
        para.add_citation(vec!["Smi20".into()], "[1]".into());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(zip_has_entry(&buf, "customXml/item1.xml"), "expected customXml/item1.xml");
        let xml = read_zip_entry(&buf, "customXml/item1.xml");
        assert!(xml.contains("b:Sources"), "expected b:Sources root: {xml}");
        assert!(xml.contains("<b:Tag>Smi20</b:Tag>"), "expected b:Tag: {xml}");
        assert!(xml.contains("<b:SourceType>JournalArticle</b:SourceType>"), "expected source type: {xml}");
        assert!(xml.contains("<b:Last>Smith</b:Last>"), "expected author last: {xml}");
        assert!(xml.contains("<b:First>John</b:First>"), "expected author first: {xml}");
        assert!(xml.contains("<b:Title>Example Article</b:Title>"), "expected title: {xml}");
        assert!(xml.contains("<b:Year>2020</b:Year>"), "expected year: {xml}");
        assert!(xml.contains("<b:JournalName>Journal of Examples</b:JournalName>"), "expected journal: {xml}");
    }

    #[test]
    fn bibliography_zip_has_custom_xml_parts() {
        use document::{CitationSource, SourceType};
        let mut doc = Document::new();
        doc.citation_sources.push(CitationSource {
            tag: "Test1".into(),
            source_type: SourceType::Misc,
            authors: vec![],
            title: Some("Test".into()),
            year: None, journal_name: None, volume: None, issue: None,
            pages: None, doi: None, url: None, publisher: None,
            city: None, edition: None, book_title: None,
        });
        let mut para = document::Paragraph::new();
        para.add_citation(vec!["Test1".into()], "[1]".into());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        assert!(zip_has_entry(&buf, "customXml/item1.xml"));
        assert!(zip_has_entry(&buf, "customXml/itemProps1.xml"));
        assert!(zip_has_entry(&buf, "customXml/_rels/item1.xml.rels"));

        let props = read_zip_entry(&buf, "customXml/itemProps1.xml");
        assert!(props.contains("ds:datastoreItem"), "expected datastoreItem: {props}");

        let rels = read_zip_entry(&buf, "customXml/_rels/item1.xml.rels");
        assert!(rels.contains("customXmlProps"), "expected customXmlProps rel: {rels}");

        let ct = read_zip_entry(&buf, "[Content_Types].xml");
        assert!(ct.contains("customXmlProperties"), "expected content type override: {ct}");

        let doc_rels = read_zip_entry(&buf, "word/_rels/document.xml.rels");
        assert!(doc_rels.contains("customXml"), "expected customXml relationship: {doc_rels}");
    }

    #[test]
    fn no_citations_means_no_custom_xml() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        assert!(!zip_has_entry(&buf, "customXml/item1.xml"));
    }

    // ── 49. Bibliography block SDT ────────────────────────────────────

    #[test]
    fn bibliography_block_produces_sdt_with_field() {
        let mut doc = Document::new();
        let mut bib_para = document::Paragraph::new();
        bib_para.add_run("Smith, J. (2020). Example. Journal, 42(3), 100-115.");
        bib_para.hanging_indent = true;
        doc.body.elements.push(document::BlockElement::BibliographyBlock {
            paragraphs: vec![bib_para],
        });

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("w:bibliography"), "expected w:bibliography SDT marker: {xml}");
        assert!(xml.contains("BIBLIOGRAPHY"), "expected BIBLIOGRAPHY field code: {xml}");
        assert!(xml.contains("Smith, J."), "expected cached bibliography text: {xml}");
    }

    // ── OOXML spec compliance tests ────────────────────────────────────

    #[test]
    fn source_type_thesis_serializes_as_report() {
        assert_eq!(document::SourceType::Thesis.as_ooxml_str(), "Report");
    }

    #[test]
    fn custom_xml_sources_has_guid() {
        use document::{CitationSource, SourceType};
        let mut doc = Document::new();
        doc.citation_sources.push(CitationSource {
            tag: "Test1".into(),
            source_type: SourceType::Misc,
            authors: vec![],
            title: Some("Test".into()),
            year: None, journal_name: None, volume: None, issue: None,
            pages: None, doi: None, url: None, publisher: None,
            city: None, edition: None, book_title: None,
        });
        let mut para = document::Paragraph::new();
        para.add_citation(vec!["Test1".into()], "[1]".into());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "customXml/item1.xml");
        assert!(xml.contains("<b:Guid>"), "expected b:Guid element: {xml}");
        assert!(xml.contains("<b:Guid>{"), "expected GUID format with braces: {xml}");
    }

    #[test]
    fn bibliography_sdt_has_unique_id() {
        let mut doc = Document::new();
        let mut p1 = document::Paragraph::new();
        p1.add_citation(vec!["Smi20".into()], "[1]".into());
        doc.add_paragraph(p1);
        let mut bib_para = document::Paragraph::new();
        bib_para.add_run("Smith (2020).");
        doc.body.elements.push(document::BlockElement::BibliographyBlock {
            paragraphs: vec![bib_para],
        });

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(!xml.contains("111145805"), "SDT ID should not be hardcoded: {xml}");
    }

    #[test]
    fn bibliography_sdt_has_doc_part_gallery() {
        let mut doc = Document::new();
        let mut bib_para = document::Paragraph::new();
        bib_para.add_run("Reference entry.");
        doc.body.elements.push(document::BlockElement::BibliographyBlock {
            paragraphs: vec![bib_para],
        });

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(xml.contains("w:docPartGallery"), "expected docPartGallery: {xml}");
        assert!(xml.contains("Bibliographies"), "expected Bibliographies value: {xml}");
        assert!(xml.contains("w:docPartUnique"), "expected docPartUnique: {xml}");
    }

    #[test]
    fn custom_xml_item_props_has_unique_guid() {
        use document::{CitationSource, SourceType};
        let mut doc = Document::new();
        doc.citation_sources.push(CitationSource {
            tag: "A".into(),
            source_type: SourceType::Misc,
            authors: vec![],
            title: None, year: None, journal_name: None, volume: None,
            issue: None, pages: None, doi: None, url: None,
            publisher: None, city: None, edition: None, book_title: None,
        });
        let mut para = document::Paragraph::new();
        para.add_citation(vec!["A".into()], "[1]".into());
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "customXml/itemProps1.xml");
        assert!(!xml.contains("C5A3B2D1"), "itemProps should not use hardcoded UUID: {xml}");
        assert!(xml.contains("ds:itemID=\"{"), "expected GUID format: {xml}");
    }
}
