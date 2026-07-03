# typort Architecture

This document explains how typort turns a Typst document into an editable Word
file. It is written for newcomers to the codebase. For build and contribution
rules, see [`CLAUDE.md`](../CLAUDE.md) in the repository root.

## The problem

A `.typ` file is a *program*. Running it produces a laid-out document. Converting
that to a `.docx` is hard for one reason: **the two formats disagree about what a
document *is***.

- Word is **semantic and reflowable**: a heading is a heading (style `Heading 1`),
  a footnote is a native footnote, a table is a grid of cells, an equation is an
  editable OMML object. There are no "pages" in the content model — Word paginates
  on its own.
- A rendered Typst document is **geometric**: glyphs placed at coordinates on
  fixed-size pages. By the time it is laid out, "this is a heading" has decayed
  into "these glyphs are 22pt and bold."

A naive converter renders Typst to pages (or PDF) and reconstructs Word from the
geometry. That path is lossy — headings become bold text, footnotes become
page-bottom paragraphs, cross-references break. typort exists to avoid exactly
that. (See the competitor analysis in the project notes: PDF-intermediate tools
are fundamentally lossy.)

## The core idea: read the document from three sources at once

typort compiles the **same source** more than once, because no single
representation carries everything Word needs. There are **three** information
sources, in order of authority:

| Source | Produced by | Carries | Used for |
|--------|-------------|---------|----------|
| **HTML semantics** | `typst::compile::<HtmlDocument>` | element identity + document order + an *introspector* | the skeleton: what each thing *is* and what order it comes in |
| **Paged geometry** | `typst::compile::<PagedDocument>` | fonts, sizes, colors, alignment, page dimensions, images, and content that has no HTML form | "painting on" real styles, and *recovering* content HTML dropped |
| **Source AST** | re-parsing the `.typ` text (and imports) for `set` rules | authoritative declared values (`set text(font: ...)`, margins, columns) | overriding heuristic guesses with what the author literally wrote |

The mental model:

> **HTML is the skeleton (semantics + order). Paged paints it (real styles) and
> patches it (layout-only content HTML couldn't express). The source AST corrects
> both when the author stated a value explicitly.**

### Why HTML alone is not enough

Typst's HTML export deliberately omits visual information and drops content that
has no semantic HTML representation:

- **No styling values.** `<h1>` tells you "heading"; it does *not* tell you the
  font, the exact point size, the RGB color, or the alignment. Those exist only in
  the rendered frames. typort reverse-engineers them from Paged geometry
  (`convert/page.rs`).
- **Layout-only constructs vanish.** `#align(center)[...]`, `#place(...)`, some
  `#grid(...)` layouts, and `#line()` rules have no HTML element, so they are
  *absent from the DOM*. typort recovers them by diffing rendered text lines
  against the model (`convert/recovery.rs`).
- **Page-level facts don't exist before pagination.** Page size, margins, columns,
  headers/footers (which live in the page-margin zones), page breaks, and page
  numbering only come into being once the document is laid out — i.e. only from
  Paged.
- **Math is MathML, not OMML.** The HTML target (typst 0.15) renders equations to
  **MathML** `<math>` elements — semantic, but not what Word edits. typort still
  pulls the original `EquationElem` Content tree through the **introspector** and
  converts it to OMML (`typort-math`); the HTML walk skips the `<math>` node so
  the equation's glyphs aren't re-emitted as duplicate literal text
  (`convert/mod.rs`, `"math"` arm). MathML→OMML transliteration is a possible
  future alternative, but the Content tree is the richer source today.

### Why Paged alone is not enough

The rendered document has lost the semantics. A heading is just large bold
glyphs; a footnote is just text near the bottom of a page; a table is just lines
and text boxes. Building Word from geometry alone is the lossy PDF→Word path we
are explicitly avoiding. HTML is what lets typort emit **native** Word headings,
footnotes, and cross-references.

### Why the source AST is a third source

Heuristics from geometry are a *fallback*. When the author wrote
`#set text(font: ("Times New Roman", "SimSun"))`, that declaration is more
trustworthy than counting glyphs. `convert/page.rs::extract_source_style_overrides`
re-parses the main source **and its imports** (template `lib.typ` files often hide
`set` rules inside functions) and these values override the Paged-derived guesses.

## The pipeline

`typort_core::convert::convert(world) -> Result<Document, Vec<String>>` is the
heart of the system. It is a linear, numbered pipeline. The steps are grouped here
by purpose; see the numbered comments in `crates/typort-core/src/convert/mod.rs`
for the exact order.

```
                    ┌─────────────────────────────────────────────┐
   input.typ ──►    │  TyportWorld  (World trait: source, fonts,   │
                    │                packages, Feature::Html)      │
                    └───────────────┬──────────────┬──────────────┘
                                    │              │
              compile::<HtmlDocument>      compile::<PagedDocument>
                                    │              │
                    semantics + introspector   geometry + frames
                                    │              │
                                    ▼              ▼
                    ┌─────────────────────────────────────────────┐
                    │  convert():                                   │
                    │   1-3  page setup + document style (Paged)    │
                    │   3b   AST overrides (authoritative)          │
                    │   4-7  walk HTML tags → emit BlockElements,   │
                    │        querying introspector for detail;      │
                    │        images consumed FIFO; page breaks      │
                    │        inserted from a precomputed set        │
                    │   9    headers/footers/page numbering (Paged) │
                    │   10   recover layout-only content (Paged)    │
                    │   11   title/author metadata; bibliography    │
                    │   12   per-run styles + alignment (Paged);    │
                    │        AST par(hanging-indent); heading-run   │
                    │        prop strip                             │
                    │   13-15 section breaks, rules, line-merging   │
                    │   16   coalesce adjacent equal-format runs    │
                    └───────────────┬─────────────────────────────┘
                                    ▼
                          typort_ooxml::Document   (the IR)
                                    ▼
                          writer → XML parts → ZIP
                                    ▼
                               output.docx
```

Two design properties worth knowing:

- **Graceful degradation.** Paged compilation is treated as optional
  (`paged_result.output.ok()`). Every Paged-dependent step is guarded by
  `if let Some(paged)`, so if only HTML compiles, conversion still produces a
  (less-styled) document.
- **HTML drives order.** The HTML tag walk (`walk_tags`) defines document order.
  Paged data is matched back onto it — by introspection `Location`, by FIFO queue
  (images), or by geometric y-position (recovered content).

## Crate layout

```
crates/
  typort-cli/      Thin clap binary: parse args → convert → write .docx
  typort-core/     Typst compilation + the conversion engine (the brain)
  typort-ooxml/    The OOXML document model (IR) + XML writer + ZIP packaging
  typort-math/     Typst math Content tree → OMML
  typort-presets/  Journal/style preset loading
```

### typort-core internals

| File | Responsibility |
|------|----------------|
| `world.rs` | `TyportWorld`: implements Typst's `World` trait (source, system fonts, `@preview` package download, `Feature::Html`). |
| `convert/mod.rs` | The pipeline + the HTML tag walker + most element converters. |
| `convert/page.rs` | Reverse-engineers page settings and styles from Paged geometry; parses AST `set`-rule overrides (incl. `par(hanging-indent:)`); normalizes math-fallback fonts; strips redundant heading run props. |
| `convert/recovery.rs` | Recovers layout-only content HTML dropped; page breaks; horizontal rules; same-line paragraph merging. |
| `convert/coalesce.rs` | Final pass: merges adjacent runs with identical effective `rPr` and folds whitespace-only runs, undoing the per-text-node run shattering. |
| `convert/table_width.rs` | Turns a `TableElem`'s declared column `TrackSizings` (fr/rel/auto) into per-cell `w:tcW` percentages. |
| `convert/bibliography.rs` | Citation data via the semantic `BibliographyElem` (+ re-parsing `.bib`/`.yml` with hayagriva). |
| `convert/footnote.rs` | Footnote bodies from the HTML `doc-endnotes` section. |
| `convert/image.rs` | Embedded image bytes from Paged frames; rasterizes SVG `<img>` (via `resvg`) and whole drawing canvases — CeTZ plots/diagrams, detected by a Bézier-curve signature — to PNG (via `typst-render`). |
| `convert/inline.rs` | Inline formatting (bold/italic/…) → styled runs. |

## The IR: `typort_ooxml::Document`

This is the typed contract between the two halves of the system. `typort-core`
**writes** it; `typort-ooxml::writer` **reads** it; it is the only thing they
share.

- `Document` — root: body elements, footnotes, page settings, document style,
  header/footer, page numbering, citation sources, metadata.
- `BlockElement` — `Paragraph | Table | BibliographyBlock` (the bibliography
  section, wrapped in an SDT carrying a `BIBLIOGRAPHY` field code).
- `Paragraph` — inline runs + optional `ParagraphStyle` + alignment/indent/spacing.
- `InlineElement` — `Text(Run)`, images, footnote refs, breaks, fields/bookmarks.
- `Run` — a styled text span (bold/italic/underline/color/font/size/script).
- `Table` / `TableRow` / `TableCell` — colspan/rowspan, shading, borders,
  multi-paragraph cells.
- `ParagraphStyle` — a **typed enum** (`Heading(n)`, `Normal`, `Quote`, `Code`, …)
  rather than stringly-typed style IDs.

Units are stored in **Word-native form** (half-points for size, twips for
spacing/indent) so the writer does minimal conversion.

## The writer: direct OOXML, no library

`typort-ooxml::writer` emits the `.docx` XML parts directly via `quick-xml` — **no
`docx-rs`, no intermediate format**. This is a deliberate choice: Word is strict
about WML child-element ordering (`rPr`/`pPr` children must appear in schema
order), and direct emission gives full control. The writer produces all parts
(`document.xml`, `styles.xml`, `numbering.xml`, `settings.xml`, relationships,
content-types, headers/footers, footnotes, media) and zips them.

## Math → OMML

`typort-math` converts the Typst math Content tree (obtained *semantically* via
the introspector, not from rendered glyphs) into OMML, which keeps equations
**editable** in Word. It implements a broad set of OMML constructs: fractions
(`m:f`), scripts (`m:sSub`/`m:sSup`/`m:sSubSup`), pre-scripts (`m:sPre`), radicals
(`m:rad`), n-ary operators (`m:nary`), delimiters (`m:d`), matrices (`m:m`),
aligned equations (`m:eqArr`), accents (`m:acc`), bars (`m:bar`), group characters
/ over-under braces (`m:groupChr`), named functions (`m:func`), limits
(`m:limLow`/`m:limUpp`), phantoms (`m:phant`), and boxes (`m:box`). Math **style
wrappers** (e.g. `bold`, `bb`, `cal`, `upright`, `dif`) are resolved to their
styled Unicode glyphs via the `codex` crate Typst itself uses (`bb(R)` → ℝ,
`bold(e)` → 𝒆), with an explicit `m:sty="p"` forcing upright for `upright`/`dif`.

An n-ary operator's operand (the integrand/summand) is bound into its `<m:e>`
body rather than left as detached siblings: Typst stores the operand as flat
content following the operator, so the sequence walker consumes following items
into the operand until a **Relation-class** symbol (`=`, `<`, `→`, …) — using the
same `unicode-math-class` table Typst uses, so the boundary matches Typst's own
classification (e.g. in `sum_i a_i = S`, the `= S` stays outside the n-ary).

**Hard limits** (OMML / Word constraints, not bugs):

- No color inside math zones.
- No extensible arrows.
- Word forces the Cambria Math font in math zones.

## The fragile seam: `recovery.rs`

The honest part of this document. The recovery layer is where typort does, in
miniature, the very PDF→Word inference it set out to avoid — because some content
(`#align(center)`, `#place`, grids) reaches the model *only* as geometry.

`recover_missing_content` extracts every rendered text line with its position,
then **text-diffs** it against what already made it into the model, inserting
genuinely-missing lines at the geometrically-correct slot. Its correctness depends
on:

- **Text normalization matching** across two very different pipelines (strip CJK
  spaces, strip math italics, strip visual markers, strip heading numbering).
- **Magic thresholds**: minimum line lengths (2/5/6/8 chars), a math-character
  ratio (`math*4 > total`), an ~85% page-fullness heuristic for implicit page
  breaks, a 15%-of-page-center alignment threshold, a 2.0pt y-position tolerance,
  a default 0.66 cap-height ratio, and a "first 3 pages" sampling window for body
  style.

These heuristics are individually justified but collectively brittle: both false
negatives (real content skipped) and false positives (content duplicated) are
possible. Most of the project's known edge-case bugs live here. **Anyone touching
this file should add a fixture-based regression test for the specific case.**

## Language neutrality (how it stays universal)

typort's stated goal is a **universal** Typst→Word converter — it must not bake
in assumptions about a document's language or genre. Earlier versions did, in
three places; all now derive from semantics instead, and they stand as the
pattern to follow:

1. **Bibliographies** are detected via the semantic `doc-bibliography` role from
   `#bibliography(...)`, not by matching a `参考文献`/`References` heading. A
   hand-written reference list is, to Typst, ordinary text — and is converted as
   such.
2. **Figure/table captions** are deduplicated during recovery against the text
   the semantic figure path already emitted, not by matching `表 `/`图 `/`Table `/
   `Figure ` prefixes.
3. **Document language** (`w:lang`) is derived from `#set text(lang:, region:)`,
   not guessed from the presence of CJK glyphs.

If a future change reintroduces `if text.contains("<word>")` to drive layout,
it regresses this principle. See `CLAUDE.md`.
