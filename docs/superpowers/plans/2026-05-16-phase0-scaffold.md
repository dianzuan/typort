# Phase 0: Project Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up a fully functional Cargo workspace with 5 crates, engineering config, and an end-to-end test that compiles a `.typ` file and produces a valid `.docx` ZIP.

**Architecture:** Cargo workspace under `crates/` with `typort-cli` (binary) → `typort-core` (lib, Typst compilation + Content tree traversal) → `typort-ooxml` (lib, XML generation + ZIP packaging). `typort-math` and `typort-presets` are empty shells for now. The OOXML layer uses pure `quick-xml` (no docx-rs).

**Tech Stack:** Rust 2024 edition, typst 0.14.2, typst-kit 0.14.2, quick-xml 0.37, zip 2.x, clap 4.x, serde 1.x, toml 0.8.x

---

## File Map

| File | Responsibility |
|------|----------------|
| `Cargo.toml` | Workspace root, shared dependency versions |
| `rustfmt.toml` | Formatting config |
| `clippy.toml` | Lint config |
| `.gitignore` | Ignore rules |
| `.github/workflows/ci.yml` | CI pipeline |
| `CLAUDE.md` | Project dev guide for AI assistants |
| `crates/typort-cli/Cargo.toml` | Binary crate manifest |
| `crates/typort-cli/src/main.rs` | CLI entry point with clap |
| `crates/typort-core/Cargo.toml` | Core lib manifest |
| `crates/typort-core/src/lib.rs` | Public API: `compile_and_convert()` |
| `crates/typort-core/src/world.rs` | `TyportWorld` — minimal World trait impl |
| `crates/typort-core/src/convert.rs` | Content tree traversal + element dispatch |
| `crates/typort-ooxml/Cargo.toml` | OOXML lib manifest |
| `crates/typort-ooxml/src/lib.rs` | Public API: `Document` builder |
| `crates/typort-ooxml/src/document.rs` | OOXML document model (paragraphs, runs) |
| `crates/typort-ooxml/src/writer.rs` | XML serialization + ZIP packaging |
| `crates/typort-math/Cargo.toml` | Math lib manifest (shell) |
| `crates/typort-math/src/lib.rs` | Empty shell |
| `crates/typort-presets/Cargo.toml` | Presets lib manifest (shell) |
| `crates/typort-presets/src/lib.rs` | Empty shell |
| `tests/fixtures/hello.typ` | Test input file |
| `tests/integration_test.rs` | End-to-end test |
| `presets/README.md` | Placeholder for journal presets |

---

### Task 1: Initialize Git Repository and Workspace Root

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `rustfmt.toml`
- Create: `clippy.toml`

- [ ] **Step 1: Initialize git repo**

```bash
cd /home/diamondrill/workspace/typort
git init
```

- [ ] **Step 2: Create workspace root Cargo.toml**

```toml
[workspace]
members = [
    "crates/typort-cli",
    "crates/typort-core",
    "crates/typort-ooxml",
    "crates/typort-math",
    "crates/typort-presets",
]
resolver = "2"

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
repository = "https://github.com/user/typort"

[workspace.dependencies]
# Typst ecosystem
typst = "0.14.2"
typst-library = "0.14.2"
typst-syntax = "0.14.2"
typst-kit = { version = "0.14.2", features = ["fonts", "embed-fonts"] }
typst-utils = "0.14.2"

# OOXML generation
quick-xml = "0.37"
zip = { version = "2", default-features = false, features = ["deflate"] }

# Serialization
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# CLI
clap = { version = "4", features = ["derive"] }

# Internal crates
typort-core = { path = "crates/typort-core" }
typort-ooxml = { path = "crates/typort-ooxml" }
typort-math = { path = "crates/typort-math" }
typort-presets = { path = "crates/typort-presets" }
```

- [ ] **Step 3: Create .gitignore**

```gitignore
/target
*.docx
.idea/
.vscode/
*.swp
*.swo
*~
.DS_Store
```

- [ ] **Step 4: Create rustfmt.toml**

```toml
edition = "2024"
max_width = 100
```

- [ ] **Step 5: Create clippy.toml**

```toml
too-many-arguments-threshold = 8
```

Note: Pedantic lints will be enabled per-crate via `#![warn(clippy::pedantic)]` in lib.rs/main.rs with specific allows.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml .gitignore rustfmt.toml clippy.toml
git commit -m "init: workspace root with shared dependencies"
```

---

### Task 2: Create Shell Crates (typort-math, typort-presets)

**Files:**
- Create: `crates/typort-math/Cargo.toml`
- Create: `crates/typort-math/src/lib.rs`
- Create: `crates/typort-presets/Cargo.toml`
- Create: `crates/typort-presets/src/lib.rs`

- [ ] **Step 1: Create typort-math crate**

`crates/typort-math/Cargo.toml`:
```toml
[package]
name = "typort-math"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
quick-xml.workspace = true
```

`crates/typort-math/src/lib.rs`:
```rust
#![warn(clippy::pedantic)]
```

- [ ] **Step 2: Create typort-presets crate**

`crates/typort-presets/Cargo.toml`:
```toml
[package]
name = "typort-presets"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
toml.workspace = true
```

`crates/typort-presets/src/lib.rs`:
```rust
#![warn(clippy::pedantic)]
```

- [ ] **Step 3: Create presets directory placeholder**

`presets/README.md`:
```markdown
# Journal Presets

TOML configuration files for journal formatting requirements.
Will be populated in Phase 5.
```

- [ ] **Step 4: Commit**

```bash
git add crates/typort-math crates/typort-presets presets
git commit -m "init: add typort-math and typort-presets shell crates"
```

---

### Task 3: Create typort-ooxml Crate with Minimal Document Model

**Files:**
- Create: `crates/typort-ooxml/Cargo.toml`
- Create: `crates/typort-ooxml/src/lib.rs`
- Create: `crates/typort-ooxml/src/document.rs`
- Create: `crates/typort-ooxml/src/writer.rs`

- [ ] **Step 1: Write a test for minimal .docx generation**

Create `crates/typort-ooxml/Cargo.toml`:
```toml
[package]
name = "typort-ooxml"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
quick-xml.workspace = true
zip.workspace = true
```

Create `crates/typort-ooxml/src/lib.rs`:
```rust
#![warn(clippy::pedantic)]

pub mod document;
pub mod writer;

pub use document::Document;
pub use writer::write_docx;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn minimal_docx_is_valid_zip_with_content_types() {
        let doc = Document::new();
        let mut buf = Vec::new();
        write_docx(&doc, Cursor::new(&mut buf)).unwrap();

        let reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        let names: Vec<&str> = reader.file_names().collect();
        assert!(names.contains(&"[Content_Types].xml"));
        assert!(names.contains(&"word/document.xml"));
        assert!(names.contains(&"_rels/.rels"));
        assert!(names.contains(&"word/_rels/document.xml.rels"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p typort-ooxml
```

Expected: compilation error — `document` and `writer` modules don't exist yet.

- [ ] **Step 3: Implement document model**

Create `crates/typort-ooxml/src/document.rs`:
```rust
#[derive(Debug, Default)]
pub struct Document {
    pub body: Body,
}

#[derive(Debug, Default)]
pub struct Body {
    pub elements: Vec<BlockElement>,
}

#[derive(Debug)]
pub enum BlockElement {
    Paragraph(Paragraph),
}

#[derive(Debug, Default)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

#[derive(Debug)]
pub struct Run {
    pub text: String,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_paragraph(&mut self, para: Paragraph) {
        self.body.elements.push(BlockElement::Paragraph(para));
    }
}

impl Paragraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_run(&mut self, text: impl Into<String>) -> &mut Self {
        self.runs.push(Run { text: text.into() });
        self
    }
}
```

- [ ] **Step 4: Implement writer (XML serialization + ZIP packaging)**

Create `crates/typort-ooxml/src/writer.rs`:
```rust
use std::io::{Seek, Write};

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::document::{BlockElement, Document};

pub fn write_docx<W: Write + Seek>(doc: &Document, dest: W) -> Result<(), Box<dyn std::error::Error>> {
    let mut zip = ZipWriter::new(dest);
    let options = SimpleFileOptions::default();

    write_content_types(&mut zip, &options)?;
    write_rels(&mut zip, &options)?;
    write_document_rels(&mut zip, &options)?;
    write_document_xml(&mut zip, &options, doc)?;

    zip.finish()?;
    Ok(())
}

fn write_content_types<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    options: &SimpleFileOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    zip.start_file("[Content_Types].xml", *options)?;
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))?;

    let mut types = BytesStart::new("Types");
    types.push_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types"));
    writer.write_event(Event::Start(types))?;

    let mut default_rels = BytesStart::new("Default");
    default_rels.push_attribute(("Extension", "rels"));
    default_rels.push_attribute(("ContentType", "application/vnd.openxmlformats-package.relationships+xml"));
    writer.write_event(Event::Empty(default_rels))?;

    let mut default_xml = BytesStart::new("Default");
    default_xml.push_attribute(("Extension", "xml"));
    default_xml.push_attribute(("ContentType", "application/xml"));
    writer.write_event(Event::Empty(default_xml))?;

    let mut override_doc = BytesStart::new("Override");
    override_doc.push_attribute(("PartName", "/word/document.xml"));
    override_doc.push_attribute(("ContentType", "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"));
    writer.write_event(Event::Empty(override_doc))?;

    writer.write_event(Event::End(BytesEnd::new("Types")))?;
    zip.write_all(&writer.into_inner())?;
    Ok(())
}

fn write_rels<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    options: &SimpleFileOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    zip.start_file("_rels/.rels", *options)?;
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships"));
    writer.write_event(Event::Start(rels))?;

    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute(("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"));
    rel.push_attribute(("Target", "word/document.xml"));
    writer.write_event(Event::Empty(rel))?;

    writer.write_event(Event::End(BytesEnd::new("Relationships")))?;
    zip.write_all(&writer.into_inner())?;
    Ok(())
}

fn write_document_rels<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    options: &SimpleFileOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    zip.start_file("word/_rels/document.xml.rels", *options)?;
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute(("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships"));
    writer.write_event(Event::Start(rels))?;
    writer.write_event(Event::End(BytesEnd::new("Relationships")))?;

    zip.write_all(&writer.into_inner())?;
    Ok(())
}

fn write_document_xml<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    options: &SimpleFileOptions,
    doc: &Document,
) -> Result<(), Box<dyn std::error::Error>> {
    zip.start_file("word/document.xml", *options)?;
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))?;

    let mut w_document = BytesStart::new("w:document");
    w_document.push_attribute(("xmlns:w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"));
    w_document.push_attribute(("xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"));
    writer.write_event(Event::Start(w_document))?;

    writer.write_event(Event::Start(BytesStart::new("w:body")))?;

    for element in &doc.body.elements {
        match element {
            BlockElement::Paragraph(para) => write_paragraph(&mut writer, para)?,
        }
    }

    writer.write_event(Event::End(BytesEnd::new("w:body")))?;
    writer.write_event(Event::End(BytesEnd::new("w:document")))?;

    zip.write_all(&writer.into_inner())?;
    Ok(())
}

fn write_paragraph(
    writer: &mut Writer<Vec<u8>>,
    para: &Paragraph,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::document::Paragraph;

    writer.write_event(Event::Start(BytesStart::new("w:p")))?;
    for run in &para.runs {
        writer.write_event(Event::Start(BytesStart::new("w:r")))?;
        writer.write_event(Event::Start(BytesStart::new("w:t")))?;
        writer.write_event(Event::Text(BytesText::new(&run.text)))?;
        writer.write_event(Event::End(BytesEnd::new("w:t")))?;
        writer.write_event(Event::End(BytesEnd::new("w:r")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("w:p")))?;
    Ok(())
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test -p typort-ooxml
```

Expected: PASS — `minimal_docx_is_valid_zip_with_content_types`

- [ ] **Step 6: Add a test for paragraph content**

Add to `crates/typort-ooxml/src/lib.rs` test module:
```rust
    #[test]
    fn docx_contains_paragraph_text() {
        let mut doc = Document::new();
        let mut para = document::Paragraph::new();
        para.add_run("你好世界");
        doc.add_paragraph(para);

        let mut buf = Vec::new();
        write_docx(&doc, Cursor::new(&mut buf)).unwrap();

        let reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
        let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
        assert!(doc_xml.contains("你好世界"));
    }
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p typort-ooxml
```

Expected: both tests PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/typort-ooxml
git commit -m "feat(ooxml): minimal OOXML document model and .docx writer"
```

---

### Task 4: Create typort-core with Typst World Implementation

**Files:**
- Create: `crates/typort-core/Cargo.toml`
- Create: `crates/typort-core/src/lib.rs`
- Create: `crates/typort-core/src/world.rs`

- [ ] **Step 1: Write a test for Typst compilation**

Create `crates/typort-core/Cargo.toml`:
```toml
[package]
name = "typort-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
typst.workspace = true
typst-library.workspace = true
typst-syntax.workspace = true
typst-kit.workspace = true
typst-utils.workspace = true
typort-ooxml.workspace = true
typort-math.workspace = true
typort-presets.workspace = true
```

Create `crates/typort-core/src/lib.rs`:
```rust
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod world;

#[cfg(test)]
mod tests {
    use super::world::TyportWorld;
    use std::path::Path;

    #[test]
    fn compile_hello_typ() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let result = typst::compile(&world);
        assert!(result.output.is_ok(), "compilation failed: {:?}", result.output.err());
    }
}
```

- [ ] **Step 2: Create test fixture**

Create `tests/fixtures/hello.typ`:
```typst
= Hello World

This is a test document for typort.

这是一个测试文档。
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cargo test -p typort-core
```

Expected: compilation error — `world` module doesn't exist.

- [ ] **Step 4: Implement TyportWorld**

Create `crates/typort-core/src/world.rs`:
```rust
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::Library;

pub struct TyportWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    source: Source,
    root: PathBuf,
    fonts: Vec<Font>,
}

impl TyportWorld {
    pub fn new(main_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let main_path = main_path.canonicalize()?;
        let root = main_path.parent().unwrap_or(Path::new(".")).to_owned();
        let content = std::fs::read_to_string(&main_path)?;
        let source = Source::detached(content);

        let fonts = typst_kit::fonts::Fonts::searcher()
            .include_system_fonts(true)
            .search();

        let book = LazyHash::new(fonts.book.clone());
        let font_list: Vec<Font> = fonts.fonts;

        Ok(Self {
            library: LazyHash::new(Library::default()),
            book,
            source,
            root,
            fonts: font_list,
        })
    }
}

impl typst::World for TyportWorld {
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
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.root.join(id.vpath().as_rootless_path());
        let bytes = std::fs::read(&path)
            .map_err(|_| FileError::NotFound(path))?;
        Ok(Bytes::from(bytes))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}
```

**Important:** The exact `typst_kit::fonts::Fonts` API may differ from what's shown here. After writing this file, run `cargo check -p typort-core` and adjust based on compiler errors. The key patterns to look for in the typst-kit docs:
- A `FontSearcher` or `Fonts` struct with a `search()` or `build()` method
- It should produce a `FontBook` and a `Vec<Font>`

If the API doesn't match, check `typst-kit` docs or source. The World trait signature is stable at `typst 0.14.2`.

- [ ] **Step 5: Run test**

```bash
cargo test -p typort-core compile_hello_typ
```

Expected: PASS — the `.typ` file compiles successfully.

If there are compilation errors related to the `typst_kit::fonts` API, fix them based on compiler messages. Common adjustments:
- The font searcher API may use builder pattern differently
- `FontBook` may be wrapped in `Arc` or `LazyHash` from typst-kit directly

- [ ] **Step 6: Commit**

```bash
git add crates/typort-core tests/fixtures
git commit -m "feat(core): TyportWorld implementation for Typst compilation"
```

---

### Task 5: Add Content Tree Traversal to typort-core

**Files:**
- Create: `crates/typort-core/src/convert.rs`
- Modify: `crates/typort-core/src/lib.rs`

- [ ] **Step 1: Write a test for content tree traversal producing OOXML Document**

Add to `crates/typort-core/src/lib.rs`:
```rust
pub mod convert;

// Add to existing tests module:
    #[test]
    fn compile_and_convert_produces_document() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let paged = typst::compile(&world).output.unwrap();
        let doc = convert::convert_document(&paged);
        assert!(!doc.body.elements.is_empty(), "document should have at least one paragraph");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p typort-core compile_and_convert_produces_document
```

Expected: compilation error — `convert` module doesn't exist.

- [ ] **Step 3: Implement minimal content tree converter**

Create `crates/typort-core/src/convert.rs`:
```rust
use typst::layout::PagedDocument;
use typort_ooxml::document::{Document, Paragraph};

pub fn convert_document(paged: &PagedDocument) -> Document {
    let mut doc = Document::new();

    // Phase 0: extract text content from each page frame as proof-of-concept.
    // Full semantic conversion (headings, footnotes, etc.) comes in Phase 1-2.
    for page in paged.pages.iter() {
        extract_text_from_frame(&page.frame, &mut doc);
    }

    if doc.body.elements.is_empty() {
        let mut para = Paragraph::new();
        para.add_run("");
        doc.add_paragraph(para);
    }

    doc
}

fn extract_text_from_frame(frame: &typst::layout::Frame, doc: &mut Document) {
    for (_, item) in frame.items() {
        match item {
            typst::layout::FrameItem::Text(text_item) => {
                let mut para = Paragraph::new();
                let text: String = text_item.glyphs.iter().map(|g| g.c).collect();
                if !text.is_empty() {
                    para.add_run(&text);
                    doc.add_paragraph(para);
                }
            }
            typst::layout::FrameItem::Group(group) => {
                extract_text_from_frame(&group.frame, doc);
            }
            _ => {}
        }
    }
}
```

**Important:** The exact Typst frame/item API at 0.14.2 may differ. Key types to verify:
- `PagedDocument` has a `.pages` field of type `Vec<Page>` where each `Page` has a `.frame`
- `Frame` has an `.items()` method yielding `(Point, &FrameItem)` tuples
- `FrameItem::Text` contains a struct with a `.glyphs` field
- Each glyph has a `.c: char` field

If any of these don't compile, check `typst::layout` docs. The core pattern (recursively walk frames, collect text) is correct even if field names differ.

- [ ] **Step 4: Run test**

```bash
cargo test -p typort-core compile_and_convert_produces_document
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/typort-core/src/convert.rs crates/typort-core/src/lib.rs
git commit -m "feat(core): minimal content tree traversal extracting text from frames"
```

---

### Task 6: Create typort-cli with End-to-End Pipeline

**Files:**
- Create: `crates/typort-cli/Cargo.toml`
- Create: `crates/typort-cli/src/main.rs`

- [ ] **Step 1: Create CLI crate manifest**

Create `crates/typort-cli/Cargo.toml`:
```toml
[package]
name = "typort-cli"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[[bin]]
name = "typort"
path = "src/main.rs"

[dependencies]
clap.workspace = true
typort-core.workspace = true
typort-ooxml.workspace = true
```

- [ ] **Step 2: Implement CLI entry point**

Create `crates/typort-cli/src/main.rs`:
```rust
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "typort", about = "Convert Typst documents to Word (.docx)")]
struct Cli {
    /// Input .typ file
    input: PathBuf,

    /// Output .docx file
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let output_path = cli.output.unwrap_or_else(|| cli.input.with_extension("docx"));

    let world = typort_core::world::TyportWorld::new(&cli.input).unwrap_or_else(|e| {
        eprintln!("error: failed to read input file: {e}");
        std::process::exit(1);
    });

    let compiled = typst::compile(&world);
    let paged = compiled.output.unwrap_or_else(|diagnostics| {
        eprintln!("error: Typst compilation failed:");
        for diag in &diagnostics {
            eprintln!("  {diag:?}");
        }
        std::process::exit(1);
    });

    let doc = typort_core::convert::convert_document(&paged);

    let file = File::create(&output_path).unwrap_or_else(|e| {
        eprintln!("error: cannot create output file: {e}");
        std::process::exit(1);
    });

    typort_ooxml::write_docx(&doc, file).unwrap_or_else(|e| {
        eprintln!("error: failed to write .docx: {e}");
        std::process::exit(1);
    });

    println!("wrote {}", output_path.display());
}
```

- [ ] **Step 3: Build and test the CLI**

```bash
cargo build -p typort-cli
cargo run -p typort-cli -- tests/fixtures/hello.typ -o /tmp/hello_test.docx
```

Expected: "wrote /tmp/hello_test.docx" and a valid .docx file.

- [ ] **Step 4: Verify output is valid**

```bash
python3 -c "
import zipfile, sys
z = zipfile.ZipFile('/tmp/hello_test.docx')
names = z.namelist()
assert '[Content_Types].xml' in names, f'Missing Content_Types, got: {names}'
assert 'word/document.xml' in names, f'Missing document.xml, got: {names}'
print('Valid .docx structure')
doc = z.read('word/document.xml').decode()
print(f'document.xml length: {len(doc)} bytes')
print('Contains Chinese text:', '测试' in doc or '文档' in doc)
"
```

Expected: "Valid .docx structure" and Chinese text present.

- [ ] **Step 5: Commit**

```bash
git add crates/typort-cli
git commit -m "feat(cli): typort CLI with end-to-end typ-to-docx pipeline"
```

---

### Task 7: Integration Test

**Files:**
- Create: `tests/integration_test.rs`

- [ ] **Step 1: Write the integration test**

Create `tests/integration_test.rs`:
```rust
use std::io::Cursor;
use std::path::Path;

#[test]
fn end_to_end_hello_typ_to_docx() {
    let world =
        typort_core::world::TyportWorld::new(Path::new("tests/fixtures/hello.typ")).unwrap();
    let compiled = typst::compile(&world);
    let paged = compiled.output.expect("hello.typ should compile without errors");
    let doc = typort_core::convert::convert_document(&paged);

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<&str> = reader.file_names().collect();

    assert!(
        names.contains(&"[Content_Types].xml"),
        "missing [Content_Types].xml"
    );
    assert!(
        names.contains(&"word/document.xml"),
        "missing word/document.xml"
    );
    assert!(names.contains(&"_rels/.rels"), "missing _rels/.rels");

    let doc_xml =
        std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("w:document"),
        "document.xml should contain w:document element"
    );
}
```

Note: The root `Cargo.toml` needs a `[dev-dependencies]` section for the integration test to access the crates. Add to the root `Cargo.toml`:
```toml
[dev-dependencies]
typort-core = { path = "crates/typort-core" }
typort-ooxml = { path = "crates/typort-ooxml" }
typst = "0.14.2"
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 2: Run the integration test**

```bash
cargo test --test integration_test
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/integration_test.rs Cargo.toml
git commit -m "test: end-to-end integration test (typ → docx → verify ZIP)"
```

---

### Task 8: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create CI workflow**

Create `.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check
```

- [ ] **Step 2: Commit**

```bash
git add .github
git commit -m "ci: GitHub Actions workflow (check, clippy, test, fmt)"
```

---

### Task 9: CLAUDE.md Project Guide

**Files:**
- Create: `CLAUDE.md`

- [ ] **Step 1: Create CLAUDE.md**

Create `CLAUDE.md`:
```markdown
# typort

Typst → Word (.docx) converter targeting Chinese social science papers.

## Build & Test

```bash
cargo build --workspace        # Build all crates
cargo test --workspace         # Run all tests
cargo run -p typort-cli -- input.typ -o output.docx  # Run CLI
```

## Architecture

Cargo workspace with 5 crates under `crates/`:

- `typort-cli` — Binary. CLI entry point (clap). Depends on typort-core.
- `typort-core` — Lib. Typst compilation (World impl), Content tree traversal, element dispatch. Depends on typort-ooxml, typort-math, typort-presets.
- `typort-ooxml` — Lib. OOXML XML generation (pure quick-xml) + ZIP packaging.
- `typort-math` — Lib. Typst math Content → OMML conversion. (Phase 3, currently empty)
- `typort-presets` — Lib. Journal preset TOML loading. (Phase 5, currently empty)

## Key Dependencies

- `typst` 0.14.2 — Compiler crate, provides Content tree
- `typst-kit` 0.14.2 — Font discovery helpers
- `quick-xml` 0.37 — XML serialization (we do NOT use docx-rs)
- `zip` 2.x — .docx ZIP packaging

## Conventions

- Rust 2024 edition
- `#![warn(clippy::pedantic)]` in all crate roots
- Tests go in-module for unit tests, `tests/` for integration tests
- Test fixtures in `tests/fixtures/`
- No docx-rs — all OOXML XML is generated via quick-xml for full control
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add CLAUDE.md project guide"
```

---

### Task 10: Final Verification

- [ ] **Step 1: Run full build with no warnings**

```bash
cargo build --workspace 2>&1
```

Expected: compiles cleanly with no warnings.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 4: Run format check**

```bash
cargo fmt --all -- --check
```

Expected: no formatting issues.

- [ ] **Step 5: Verify CLI end-to-end**

```bash
cargo run -p typort-cli -- tests/fixtures/hello.typ -o /tmp/final_test.docx && echo "SUCCESS"
```

Expected: "wrote /tmp/final_test.docx" followed by "SUCCESS".

- [ ] **Step 6: Fix any issues found, commit if needed**

If any of the above steps fail, fix the issues and create a new commit for the fixes.
