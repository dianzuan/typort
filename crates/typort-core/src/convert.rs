//! Phase 0 content tree traversal: walks Typst `PagedDocument` frames and
//! extracts visible text into an OOXML `Document`.

use typort_ooxml::document::{Document, Paragraph};
use typst::layout::{Frame, FrameItem, PagedDocument};

/// Convert a compiled Typst `PagedDocument` into an OOXML `Document` by
/// extracting all visible text.
///
/// This is a Phase 0 proof-of-concept: it collects text items from each page
/// into paragraphs without any semantic analysis (headings, footnotes, etc.).
pub fn convert_document(paged: &PagedDocument) -> Document {
    let mut doc = Document::new();

    for page in &paged.pages {
        let mut text_buf = String::new();
        extract_text_from_frame(&page.frame, &mut text_buf);

        if !text_buf.is_empty() {
            let mut para = Paragraph::new();
            para.add_run(&text_buf);
            doc.add_paragraph(para);
        }
    }

    doc
}

/// Recursively walk a frame and append all text items to the buffer.
fn extract_text_from_frame(frame: &Frame, buf: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Text(text_item) => {
                buf.push_str(&text_item.text);
            }
            FrameItem::Group(group) => {
                extract_text_from_frame(&group.frame, buf);
            }
            _ => {}
        }
    }
}
