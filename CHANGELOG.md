# Changelog

All notable changes to typort are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] — unreleased

Baseline moved from typst 0.14.2 to **typst 0.15.1**. HTML export in 0.15 emits
MathML equations, groups paragraphs differently, and embeds every image as a
data URL, so the semantic skeleton typort reads is materially better than
before. The 0.14 line is no longer supported.

### Fixed

- Inline `#context [...]` inside a paragraph no longer emits the paragraph twice
  with the contextual content stripped from the second copy (#4). Numbered items
  driven by counters/state in templates now come out once, with their numbers.
- In-text citations render as citation markers with clickable cross-references to
  the bibliography instead of broken `REF` fields; the bibliography is no longer
  rendered as a bulleted list.
- Display equations are numbered the Word-native way; the differential `d`
  dropped from OMML is recovered; OMML is emitted compact (no pretty-print
  whitespace leaking into math runs).
- Super/subscript use `vertAlign` alone instead of a shrunk font size; footnotes
  are sized from the footnote body, not the superscript marker; reference markers
  have a uniform size.
- Headings: per-run bold/size that duplicated the Heading style are dropped, Word
  owns heading flow and line spacing; heading numbers, smart quotes and
  left-alignment are preserved; recovered headings are deduplicated semantically
  (the two hardcoded CJK-numeral tables are gone).
- Tables: cells are no longer indented; three-line vs boxed borders are detected
  from the drawn rules; table cell alignment is carried into Word; column widths
  honor `(1fr, 2fr, …)` / relative track sizes.
- Lists: each ordered list restarts its numbering; a conclusion section following
  a list stays intact.
- Recovery (paged-geometry fallback) no longer scrapes footnotes / separators into
  the body, duplicates a wrapped table row, injects orphan lines, or doubles
  existing whitespace when joining clusters; abstracts are restored.
- Variable fonts: bold and italic are derived from the instantiated `wght` /
  `ital` / `slnt` axis coordinates instead of the family's static style flags.
- Multiple bibliographies: citation sources are collected from every
  `BibliographyElem`; duplicate keys keep the first occurrence and emit a warning;
  citation sources are deduplicated by tag (duplicate Tag+GUID was invalid Word
  data).
- Images: unsupported formats are re-encoded and the image FIFO stays aligned with
  the document order.
- `World::today` returns the real date; built-in presets resolve next to the
  executable; page column count is taken from the source, not a geometric guess;
  only explicit `#pagebreak()` produces a page break.
- No literal space is inserted between CJK text and inline math.

### Added

- `#set par(hanging-indent:)` and `#set par(first-line-indent: (amount, all: true))`
  are honored from the source AST.
- Vector-drawing figures (CeTZ canvases, diagrams) are rasterized via
  `typst-render`; SVG via `resvg`.
- Styled math glyphs (`bold` / `bb` / `cal` / `upright` / `dif`) and equations in
  table cells.
- n-ary operands are bound into `m:e`, bounded by relation symbols.
- Adjacent equally-formatted runs are coalesced; math-fallback fonts are
  normalized.
- Code-block style carries background shading.
- Golden `word/document.xml` snapshots for a curated, CI-safe fixture set, and
  visual-regression tests (LibreOffice + pdftoppm + ImageMagick, opt-in via
  `--ignored`) that fail loudly when a tool is missing.

### Changed

- Journal presets are no longer distributed with typort. `--preset <name>` still
  loads a user-supplied `presets/<name>.toml` from next to the executable or the
  working directory.
- Requires typst 0.15.1 crates. Documents that depend on 0.14-only behaviour may
  render differently.
- Integration tests are split into one module per area under
  `crates/typort-cli/tests/integration/`.

## [0.1.1] — 2026-05-21

- Show-rule style recovery (per-run font, size, color, bold, italic from the
  paged rendering) and source-AST parsing for `#set text` / `#set par` /
  `#set page`.
- Column breaks, table cell shading / dashed borders / vertical alignment, small
  caps, text tracking, CJK font mixing, footnote tab formatting, footnotes in
  headings and TOC, inline box fill, metadata case deduplication.
- License changed to Apache-2.0.

## [0.1.0]

- Initial release: headings, inline formatting, OMML math, tables, lists,
  footnotes, images, code blocks, cross-references, hyperlinks, page/section
  breaks, headers & footers, page numbering, columns, TOC, figure captions, CJK
  typography, document metadata, `@preview` packages.

[0.2.0]: https://github.com/dianzuan/typort/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/dianzuan/typort/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/dianzuan/typort/releases/tag/v0.1.0
