#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod world;

pub use world::{TyportWorld, compile};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn compile_hello_typ() {
        let world =
            TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let result = typst::compile::<typst::layout::PagedDocument>(&world);
        assert!(
            result.output.is_ok(),
            "compilation failed: {:?}",
            result.output.err()
        );
    }
}
