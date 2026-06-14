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

/// Build the system font book, dropping any `…-ExtB` (Unicode Extension-B) face.
///
/// Workaround for [typst#6205](https://github.com/typst/typst/issues/6205)
/// (fixed in typst 0.15): `typographic_family()` strips the `-ExtB` suffix of a
/// name like `SimSun-ExtB` as if it were an `ExtraBold` style keyword and merges
/// that face into the `SimSun` family. The family then holds two weight-400 faces
/// and `font: "SimSun"` non-deterministically selects the Ext-B face — which lacks
/// the basic CJK block — so Typst falls back to an unrelated font (e.g. `LiSu`),
/// rendering the body in the wrong typeface. Excluding Ext-B faces from the book
/// makes the base family resolve to its real face. Trade-off: drops Unicode
/// Extension-B ideographs (a rarely-used block); on typst ≥ 0.15 this is moot.
fn search_fonts_without_ext_b() -> (FontBook, Vec<FontSlot>) {
    let Fonts { book, fonts } = Fonts::searcher().include_system_fonts(true).search();
    let mut filtered_book = FontBook::new();
    let mut filtered_fonts = Vec::with_capacity(fonts.len());
    for (index, slot) in fonts.into_iter().enumerate() {
        // `book.info(index)` is the cheap, already-parsed FontInfo (no font load).
        if let Some(info) = book.info(index) {
            // Drop a Unicode Extension-B-only CJK subset face (e.g. SimSun-ExtB):
            // it covers Ext-B (U+20000…) but NOT the basic CJK block (U+4E00…).
            // typst strips its "-ExtB" suffix (mistaken for an "ExtraBold" style)
            // and merges it into the base family ("SimSun"), so that family ends up
            // holding a near-empty face that shadows the real one and makes
            // `font: "SimSun"` resolve to it (then fall back to e.g. LiSu) — the
            // resolution gets the body font wrong (typst#6205). We can't match by
            // family name (both faces now report "SimSun"), so detect the subset by
            // coverage. The real base face covers U+4E00, so it is kept.
            if info.coverage.contains(0x2_0000) && !info.coverage.contains(0x4E00) {
                continue;
            }
            filtered_book.push(info.clone());
            filtered_fonts.push(slot);
        }
    }
    (filtered_book, filtered_fonts)
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

        let (book, fonts) = search_fonts_without_ext_b();

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

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        // Per the `World` contract: `offset` is a UTC offset in hours, or `None`
        // for the local time zone. `now_local()` can fail (e.g. the time crate
        // refuses to read the zone from a multi-threaded process); fall back to
        // UTC rather than a fixed placeholder so the date is at least real.
        let now =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let resolved = match offset {
            None => now,
            Some(hours) => {
                let secs = i32::try_from(hours).ok()?.checked_mul(3600)?;
                now.to_offset(time::UtcOffset::from_whole_seconds(secs).ok()?)
            }
        };
        // `time::Month` discriminants are 1..=12 and `day()` is 1..=31 — exactly
        // the `u8` ranges Datetime::from_ymd expects.
        Datetime::from_ymd(resolved.year(), resolved.month() as u8, resolved.day())
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
