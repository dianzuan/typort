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
/// Loads both the bundled (`embedded`) and the system fonts — matching the old
/// `include_system_fonts(true)` behavior — then filters out the Ext-B subset
/// faces (see [`is_ext_b_subset`]) before they enter the store, so the font book
/// it exposes never contains them.
fn search_fonts_without_ext_b() -> FontStore {
    let mut store = FontStore::new();
    store.extend(filter_ext_b(fonts::embedded()));
    store.extend(filter_ext_b(fonts::system()));
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
        let abs_path = path.canonicalize()?;
        let root = abs_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
        let vpath = VirtualPath::new(format!("/{file_name}"))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let content = std::fs::read_to_string(&abs_path)?;
        let file_id = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let source = Source::new(file_id, content);

        let fonts = search_fonts_without_ext_b();

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
            // a `FileError` via the `From` impl, so `?` is enough.
            VirtualRoot::Package(spec) => Ok(self.packages.obtain(spec)?.resolve(id.vpath())),
            VirtualRoot::Project => Ok(id.vpath().realize(&self.root)),
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
                // `Duration::seconds()` is the total offset in seconds as `f64`;
                // round to whole seconds and reject a non-finite or out-of-`i32`
                // value (a real UTC offset is ±18h, well within range) so we never
                // truncate — out-of-range just falls back to no adjustment below.
                let rounded = duration.seconds().round();
                if !rounded.is_finite()
                    || rounded < f64::from(i32::MIN)
                    || rounded > f64::from(i32::MAX)
                {
                    return None;
                }
                // Bounds-checked just above, so this `f64 -> i32` cannot truncate.
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "rounded value verified in i32 range"
                )]
                let secs = rounded as i32;
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
