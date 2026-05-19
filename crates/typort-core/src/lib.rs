#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod convert;
pub mod world;

pub use convert::convert;
pub use world::{TyportWorld, compile};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::TyportWorld;
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
    fn convert_produces_heading_and_paragraphs() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let doc = crate::convert::convert(&world).unwrap();

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
        assert!(!p.inlines.is_empty());
        assert!(p.text_content().contains("Hello"));
    }

    #[test]
    fn complex_paper_has_multiple_headings() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let doc = crate::convert::convert(&world).unwrap();

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
}
