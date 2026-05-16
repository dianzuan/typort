#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod convert;
pub mod world;

pub use world::{TyportWorld, compile};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

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
    fn compile_and_convert_produces_document() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let paged = compile(&world).unwrap();
        let doc = convert::convert_document(&paged);
        assert!(
            !doc.body.elements.is_empty(),
            "document should have at least one element"
        );
    }
}
