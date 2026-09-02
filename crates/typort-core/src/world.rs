use std::path::{Path, PathBuf};

use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook, FontInfo};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};
use typst_kit::downloader::SystemDownloader;
use typst_kit::fonts::{self, FontSource, FontStore};
use typst_kit::packages::SystemPackages;
use typst_layout::PagedDocument;

pub struct TyportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    root: PathBuf,
    packages: SystemPackages,
}

/// Whether `info` describes a Unicode Extension-B-only CJK subset face.
///
/// Workaround for [typst#6205](https://github.com/typst/typst/issues/6205)
/// (fixed in typst 0.15): `typographic_family()` strips the `-ExtB` suffix of a
/// name like `SimSun-ExtB` as if it were an `ExtraBold` style keyword and merges
/// that face into the `SimSun` family. The family then holds two weight-400 faces
/// and `font: "SimSun"` non-deterministically selects the Ext-B face — which lacks
/// the basic CJK block — so Typst falls back to an unrelated font (e.g. `LiSu`),
/// rendering the body in the wrong typeface. Excluding Ext-B faces from the store
/// makes the base family resolve to its real face. Trade-off: drops Unicode
/// Extension-B ideographs (a rarely-used block); on typst ≥ 0.15 this is moot.
///
/// We can't match by family name (both faces now report `SimSun`), so detect the
/// subset by coverage: it covers Ext-B (`U+20000`…) but NOT the basic CJK block
/// (`U+4E00`…). The real base face covers `U+4E00`, so it is kept.
fn is_ext_b_subset(info: &FontInfo) -> bool {
    info.coverage.contains(0x2_0000) && !info.coverage.contains(0x4E00)
}

/// Build the font store, dropping any `…-ExtB` (Unicode Extension-B) subset face.
///
/// Loads the bundled (`embedded`) and system fonts plus any faces found
/// (recursively) in `extra_dirs`, then filters out the Ext-B subset faces (see
/// [`is_ext_b_subset`]) before they enter the store, so the font book it exposes
/// never contains them. `extra_dirs` is empty for [`TyportWorld::new`]; it exists
/// so tests can vendor fonts under `tests/fonts/` instead of depending on what
/// happens to be installed on the system.
fn search_fonts_without_ext_b(extra_dirs: &[PathBuf]) -> FontStore {
    let mut store = FontStore::new();
    store.extend(filter_ext_b(fonts::embedded()));
    store.extend(filter_ext_b(fonts::system()));
    for dir in extra_dirs {
        store.extend(filter_ext_b(fonts::scan(dir)));
    }
    store
}

/// Drop the Ext-B subset faces from a font-source iterator (see
/// [`is_ext_b_subset`]) while leaving every other face untouched and in order.
fn filter_ext_b<T: FontSource>(
    entries: impl Iterator<Item = (T, FontInfo)>,
) -> impl Iterator<Item = (T, FontInfo)> {
    entries.filter(|(_, info)| !is_ext_b_subset(info))
}

impl TyportWorld {
    /// Create a new world that compiles the given `.typ` file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    pub fn new(path: &Path) -> std::io::Result<Self> {
        Self::with_font_dirs(path, &[])
    }

    /// Create a new world, additionally loading fonts from `font_dirs`.
    ///
    /// Fonts are searched recursively in each directory and are added on top
    /// of the embedded and system fonts `new` already loads — this is a
    /// superset, not a replacement. Intended for tests that vendor a font
    /// under `tests/fonts/` rather than relying on what happens to be
    /// installed on the machine running the test.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    pub fn with_font_dirs(path: &Path, font_dirs: &[PathBuf]) -> std::io::Result<Self> {
        let abs_path = path.canonicalize()?;
        let root = abs_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
        let vpath = VirtualPath::new(format!("/{file_name}"))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let content = std::fs::read_to_string(&abs_path)?;
        let file_id = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let source = Source::new(file_id, content);

        let fonts = search_fonts_without_ext_b(font_dirs);

        let library = Library::builder()
            .with_features([Feature::Html].into_iter().collect())
            .build();

        let downloader = SystemDownloader::new("typort");
        let packages = SystemPackages::new(downloader);

        Ok(Self {
            library: LazyHash::new(library),
            fonts,
            source,
            root,
            packages,
        })
    }

    pub fn main_source(&self) -> &Source {
        &self.source
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve_path(&self, id: FileId) -> FileResult<PathBuf> {
        match id.root() {
            // `obtain` may download the package; its `PackageError` converts into
            // a `FileError` via the `From` impl, so `?` is enough. Same for the
            // `RealizeError` from `resolve`/`realize` (0.15: paths that would
            // lexically escape the root are rejected instead of realized).
            VirtualRoot::Package(spec) => Ok(self.packages.obtain(spec)?.resolve(id.vpath())?),
            VirtualRoot::Project => Ok(id.vpath().realize(&self.root)?),
        }
    }
}

impl World for TyportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
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
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        // Per the `World` contract: `offset` is a UTC offset (as a `Duration`),
        // or `None` for the local time zone. `now_local()` can fail (e.g. the
        // time crate refuses to read the zone from a multi-threaded process);
        // fall back to UTC rather than a fixed placeholder so the date is real.
        let now =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let resolved = match offset {
            None => now,
            Some(duration) => {
                let [weeks, days, hours, minutes, seconds] = duration.decompose();
                let secs = weeks
                    .checked_mul(7)?
                    .checked_add(days)?
                    .checked_mul(24)?
                    .checked_add(hours)?
                    .checked_mul(60)?
                    .checked_add(minutes)?
                    .checked_mul(60)?
                    .checked_add(seconds)?;
                let secs = i32::try_from(secs).ok()?;
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
