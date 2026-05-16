use std::path::Path;

use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::{FontSlot, Fonts};

pub struct TyportWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
    source: Source,
}

impl TyportWorld {
    /// Create a new world that compiles the given `.typ` file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let source = Source::detached(content);

        let Fonts { book, fonts } = Fonts::searcher().include_system_fonts(false).search();

        Ok(Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            source,
        })
    }
}

impl World for TyportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(
                id.vpath().as_rootless_path().into(),
            ))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(
            id.vpath().as_rootless_path().into(),
        ))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index)?.get()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 1, 1)
    }
}

/// Compile a `.typ` file into a paged document.
///
/// # Errors
/// Returns compilation errors if the file cannot be compiled.
pub fn compile(world: &TyportWorld) -> Result<PagedDocument, Vec<String>> {
    let result = typst::compile::<PagedDocument>(world);
    match result.output {
        Ok(doc) => Ok(doc),
        Err(errors) => Err(errors.iter().map(|e| e.message.to_string()).collect()),
    }
}
