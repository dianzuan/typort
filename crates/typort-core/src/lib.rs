#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod convert;
pub mod world;

mod realize_test;

pub use convert::convert_html;
pub use world::{TyportWorld, compile};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use typort_ooxml::document::{BlockElement, ParagraphStyle};

    #[test]
    fn compile_hello_typ() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let result = typst::compile::<typst::layout::PagedDocument>(&world);
        assert!(
            result.output.is_ok(),
            "compilation failed: {:?}",
            result.output.err()
        );
    }

    #[test]
    fn convert_html_produces_heading_and_paragraphs() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let doc = convert_html(&world).unwrap();

        assert!(
            doc.body.elements.len() >= 3,
            "should have heading + 2 paragraphs, got {}",
            doc.body.elements.len()
        );

        // First element should be a heading
        let BlockElement::Paragraph(p) = &doc.body.elements[0] else {
            panic!("expected Paragraph, got Table");
        };
        assert_eq!(p.style, Some(ParagraphStyle::Heading(1)));
        assert!(!p.runs.is_empty());
        assert!(p.runs[0].text.contains("Hello"));
    }

    #[test]
    fn complex_paper_has_multiple_headings() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let doc = convert_html(&world).unwrap();

        let heading_count = doc.body.elements.iter().filter(|e| {
            matches!(e, BlockElement::Paragraph(p) if matches!(p.style, Some(ParagraphStyle::Heading(_))))
        }).count();

        assert!(
            heading_count >= 5,
            "complex paper should have at least 5 headings, got {heading_count}"
        );

        assert!(
            doc.body.elements.len() >= 20,
            "complex paper should have many elements, got {}",
            doc.body.elements.len()
        );
    }

    #[test]
    fn center_test_recovers_aligned_content() {
        use typort_ooxml::document::Alignment;

        let world = TyportWorld::new(Path::new("../../tests/fixtures/center_test.typ")).unwrap();
        let doc = convert_html(&world).unwrap();

        // The centered text "张三  李四" should be recovered from PagedDocument
        let has_centered_authors = doc.body.elements.iter().any(|e| {
            if let BlockElement::Paragraph(p) = e {
                let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
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
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let doc = convert_html(&world).unwrap();

        // The author names and institution info from #align(center) should be recovered
        let has_author = doc.body.elements.iter().any(|e| {
            if let BlockElement::Paragraph(p) = e {
                p.runs
                    .iter()
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
            if let BlockElement::Paragraph(p) = e {
                p.runs
                    .iter()
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
}
