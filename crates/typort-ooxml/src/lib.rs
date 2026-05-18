#![warn(clippy::pedantic)]

pub mod document;
pub mod styles;
pub mod writer;

pub use document::{Document, DocumentMetadata, DocumentStyle, FootnoteFormat};
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
            xml.contains(r#"w:hanging="420""#),
            "expected hanging=420 in: {xml}"
        );
        assert!(
            xml.contains(r#"w:left="420""#),
            "expected left=420 in: {xml}"
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
        para.list_id = Some(1);
        para.list_level = Some(0);
        para.add_run("list item");
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        assert!(
            xml.contains("<w:numPr>"),
            "expected w:numPr in: {xml}"
        );
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
        let fn_id = doc.add_footnote(vec![document::Run::new("note text")]);
        let mut para = document::Paragraph::new();
        para.add_run("See");
        para.add_footnote_ref(fn_id);
        doc.add_paragraph(para);

        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/document.xml");
        let expected = format!(r#"<w:footnoteReference w:id="{fn_id}"/>"#);
        assert!(
            xml.contains(&expected),
            "expected {expected} in: {xml}"
        );
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
        let fn_id = doc.add_footnote(vec![document::Run::new("circled note")]);
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
        doc.add_footnote(vec![document::Run::new("This is a footnote.")]);
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
            colspan: 1,
            vmerge: document::VMerge::None,
            width_pct: None,
        };
        let row = document::TableRow {
            cells: vec![cell],
        };
        let table = document::Table { rows: vec![row] };
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
    fn styles_xml_normal_has_jc_both_and_first_line_indent() {
        let doc = Document::new();
        let buf = build_docx(&doc);
        let xml = read_zip_entry(&buf, "word/styles.xml");
        // Normal style uses justify (both)
        assert!(
            xml.contains(r#"<w:jc w:val="both"/>"#),
            "expected jc both in Normal style: {xml}"
        );
        // Default first-line indent is 420 twips
        assert!(
            xml.contains(r#"w:firstLine="420""#),
            "expected firstLine=420 in Normal style: {xml}"
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
        doc.add_footnote(vec![document::Run::new("fn")]);
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
        assert!(
            xml.contains(omml),
            "expected OMML passthrough in: {xml}"
        );
    }

    // ── 19. Numbered equation ───────────────────────────────────────────

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
        assert!(
            xml.contains("<w:tab/>"),
            "expected tab element in: {xml}"
        );
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
}
