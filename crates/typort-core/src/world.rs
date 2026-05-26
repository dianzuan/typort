use std::path::{Path, PathBuf};

use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};
use typst_kit::download::{Downloader, ProgressSink};
use typst_kit::fonts::{FontSlot, Fonts};
use typst_kit::package::PackageStorage;

pub struct TyportWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
    source: Source,
    root: PathBuf,
    package_storage: PackageStorage,
}

impl TyportWorld {
    /// Create a new world that compiles the given `.typ` file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let abs_path = path.canonicalize()?;
        let root = abs_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
        let vpath = VirtualPath::new(format!("/{file_name}"));
        let content = std::fs::read_to_string(&abs_path)?;
        let source = Source::new(FileId::new(None, vpath), content);

        let Fonts { book, fonts } = Fonts::searcher().include_system_fonts(true).search();

        let library = Library::builder()
            .with_features([Feature::Html].into_iter().collect())
            .build();

        let downloader = Downloader::new("typort");
        let package_storage = PackageStorage::new(None, None, downloader);

        Ok(Self {
            library: LazyHash::new(library),
            book: LazyHash::new(book),
            fonts,
            source,
            root,
            package_storage,
        })
    }

    pub fn main_source(&self) -> &Source {
        &self.source
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve_path(&self, id: FileId) -> FileResult<PathBuf> {
        if let Some(spec) = id.package() {
            let package_dir = self
                .package_storage
                .prepare_package(spec, &mut ProgressSink)?;
            Ok(package_dir.join(id.vpath().as_rootless_path()))
        } else {
            Ok(self.root.join(id.vpath().as_rootless_path()))
        }
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
            let path = self.resolve_path(id)?;
            let content = std::fs::read_to_string(&path)
                .map_err(|_| typst::diag::FileError::NotFound(path))?;
            Ok(Source::new(id, content))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.resolve_path(id)?;
        let data = std::fs::read(&path).map_err(|_| typst::diag::FileError::NotFound(path))?;
        Ok(Bytes::new(data))
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
