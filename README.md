# typort

Universal Typst to Word (.docx) converter. Any valid `.typ` file should convert to an editable `.docx`.

## Install

```bash
cargo install --path crates/typort-cli
```

## Usage

```bash
typort input.typ -o output.docx
```

## What it does

typort compiles your Typst document and generates a Word file directly — no intermediate PDF, no Pandoc, no template required.

```
input.typ ──► Typst compiler ──► HtmlDocument (structure)
                               ──► PagedDocument (layout)
                                        │
                                        ▼
                              typort conversion engine
                                        │
                                        ▼
                                   output.docx
```

## Supported features

| Feature | Status |
|---------|--------|
| Headings (h1–h6) | Heading styles with detected font sizes |
| Bold, italic, underline, strikethrough, highlight, superscript, subscript, small caps | Full formatting |
| Math (inline & display) | OMML: fractions, scripts, roots, sums, integrals, matrices, accents, cases, aligned equations, overbrace/underbrace |
| Tables | colspan, rowspan, multi-paragraph cells, nested tables, cell shading, dashed borders |
| Lists | Ordered/unordered, nested up to 5+ levels, contextual spacing |
| Footnotes | Including inside table cells, with formatting and circled numbers |
| Images | PNG, JPG embedded; SVG rasterized via resvg |
| Code blocks | Detected monospace font, shading |
| Cross-references | `@label` to bookmarks + REF field codes |
| Hyperlinks | With preserved formatting (bold links, etc.) |
| Page breaks | Detected via Introspector page boundaries |
| Column breaks | `#colbreak()` to `w:br type="column"` |
| Section breaks | Auto-detected from page setting changes |
| Headers & footers | Extracted from page margin zones |
| Page numbering | `#set page(numbering: "1")` to PAGE field code |
| Columns | `#page(columns: N)` to `w:cols` |
| Table of contents | `#outline()` to TOC field code |
| Horizontal rules | `#line()` to paragraph border |
| Figure captions | Combined into single paragraph |
| Grid layouts | Recovered with tab stops |
| Show rule styling | Font, size, color, bold, italic per-run from rendered output |
| Equation numbering | Chapter-aware `(1.1)` format |
| CJK typography | Kinsoku, overflow punct, auto-spacing, justify, font mixing |
| Document metadata | Title + author from `#set document(...)` |
| Package support | `@preview/...` packages downloaded automatically |

## How it works

typort uses a **dual-compilation strategy**:

1. **HtmlDocument** — Typst's HTML export gives us semantic structure: which text is a heading, which is a footnote, where tables begin and end.

2. **PagedDocument** — Typst's page layout gives us visual properties: actual fonts, sizes, colors, alignment, page dimensions, and content that has no HTML representation (like `#align(center)[...]` or `#grid(...)`).

3. **Source AST** — typort also re-parses the `.typ` source (and its imports) for `set` rules like `#set text(font: ...)`. Values the author declared explicitly are authoritative and override the heuristics derived from rendering.

The converter walks the HTML structure for document ordering, queries the Introspector for content details, and cross-references the PagedDocument for styling. All values (fonts, sizes, spacing, indentation, alignment) are detected from the actual rendering or the source — nothing is hardcoded for any particular document. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design.

OOXML XML is generated directly via `quick-xml`. No docx-rs, no intermediate format.

## Architecture

```
crates/
  typort-cli/     CLI binary (clap)
  typort-core/    Typst compilation + conversion engine
  typort-ooxml/   OOXML document model + XML writer + ZIP packaging
  typort-math/    Typst math → OMML conversion
  typort-presets/ Journal preset loading
```

## Build & test

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Known limitations

- **OMML** does not support math coloring, extensible arrows, or strikethrough/cancel
- **Word** forces Cambria Math font in math zones
- **Ruby annotations** (`w:ruby`) — Typst 0.14.2 has no native ruby support

## License

Apache-2.0
