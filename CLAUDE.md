# CLAUDE.md

Guidance for working in this repository. This is the authoritative rule source —
when README, code comments, or memory disagree with this file, this file wins, and
the disagreement should be fixed.

typort is a **universal Typst → Word (`.docx`) converter**. Any valid `.typ`
should convert to an editable `.docx`. For how it works, read
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) first.

## Philosophy (the rules that decide design)

1. **Universal, not genre-specific.** typort converts *any* `.typ`, in *any*
   language. Never hardcode assumptions about a document's genre or language —
   no "this is a Chinese social-science paper" logic, no matching natural-language
   keywords (`参考文献`, `表`, `图`, `References`, `Abstract`, …) to drive behavior.
   When you need to identify a construct, use the **semantic Typst element**
   (`BibliographyElem`, `FigureElem`, `FootnoteElem`, caption supplement metadata),
   not rendered text. `convert/bibliography.rs` is the model to copy.
   *(Known violations exist today — see "Known debt" below. They are debt to
   remove, not patterns to follow.)*

2. **Detect, don't assume.** Styling (fonts, sizes, colors, spacing, alignment,
   margins) is read from the actual rendering or the source AST — never
   hardcoded for a document we happened to test with. Ask "does this work for
   *any* Typst document?", not "does this work for my fixture?".

3. **Semantic-first, geometry-as-fallback.** Prefer HTML semantics + the
   introspector (which preserve "this is a heading / footnote / equation"). Fall
   back to Paged geometry only for things HTML cannot express. Geometry inference
   is the lossy path we exist to avoid — keep it contained.

## Architecture in one paragraph

typort compiles the same source to **`HtmlDocument`** (semantics + document order
+ introspector) *and* **`PagedDocument`** (fonts, geometry, images, layout-only
content), and additionally **re-parses the source AST** for authoritative `set`
rules. HTML is the skeleton; Paged paints and patches it; the AST overrides both
when the author declared a value. This is **three** sources, not two — README's
"dual-compilation" wording undercounts it. Full detail in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Rust conventions

- **Edition 2024**, `max_width = 100` (`rustfmt.toml`). Run `cargo fmt` — CI fails
  on unformatted code.
- **`#![warn(clippy::pedantic)]`** per crate; **CI runs clippy with `-D warnings`**
  (`--all-targets`). Zero warnings is a hard gate, not an aspiration.
- **`#[allow(...)]` policy:** prefer the narrowest scope (item-level over
  file/crate-level) and pair each with a one-line justification. The legitimate
  bulk today is `cast_possible_truncation` / `cast_sign_loss` in layout math
  (intentional `f64 → twips/half-point`). Treat `too_many_lines` and
  `too_many_arguments` allows as **refactor signals**, not solutions (see below).
- **`unwrap()` / `expect()` / `panic!`:** this tool ingests arbitrary user input.
  Don't `unwrap` on parsed or `Option` data derived from the document.
  `unwrap()` on infallible in-memory writes (e.g. `quick-xml` to a `Vec<u8>`) is
  tolerated but prefer a documented helper. No `todo!`/`unimplemented!` in `src`.
- **Argument threading:** `convert/mod.rs` threads `html_doc`/state through ~20+
  signatures, which is why `clippy.toml` raises `too-many-arguments-threshold` to
  8. New shared state should go into a bundled context struct (a `ConvertCtx<'a>`),
  not another positional parameter. The goal is to let that threshold drop back to
  default.
- **File size:** several files are large (`convert/mod.rs`, `ooxml/writer.rs`,
  `cli/tests/integration.rs`). Don't grow them reflexively — when adding a new
  element converter or a new test area, prefer a new module/file over appending.

## Testing

- Tests are **fixture-driven**: a `.typ` file under `tests/fixtures/` is converted
  and asserted on. Add a fixture for new features.
- **Any change to `convert/recovery.rs` or the `convert/page.rs` heuristics must
  ship with a fixture-based regression test** for the specific case — that code is
  the most fragile part of the system (geometry → semantics inference with magic
  thresholds; see ARCHITECTURE.md "fragile seam").

## What must pass before committing

CI (`.github/workflows/ci.yml`) runs exactly these four jobs. Run the **same**
commands locally — match them verbatim, because small differences (e.g. adding
`--all-targets` to clippy) surface lints CI does not gate on and waste time:

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings   # NOT --all-targets; matches CI
cargo test --workspace
cargo fmt --all -- --check
```

**Toolchain caveat (verified 2026-05-30):** CI uses
`dtolnay/rust-toolchain@stable` with no `rust-toolchain.toml`, so it floats to
the latest stable Rust. A toolchain bump can turn previously-green code red
without any code change — new rustfmt layout rules or new clippy lints applied to
existing code. This bit the repo: CI went red after 2026-05-19 purely from a
toolchain move. If green-on-an-older-toolchain matters, pin it with a
`rust-toolchain.toml`. Until then, "CI is green" means "green on whatever stable
shipped today" — always re-run the four commands locally before claiming success.

Do not state a specific test count in prose anywhere (README, docs, comments) —
counts drift and become lies. Let `cargo test` report the number.

## Documentation consistency

- This file, `docs/ARCHITECTURE.md`, `README.md`, and the project memory must not
  contradict each other or the code. If you change architecture, update all of
  them in the same change.
- The project memory under
  `~/.claude/projects/.../memory/` is point-in-time and has drifted before (it
  once claimed a `direct realize` architecture that was never built, and an
  OMML coverage of "6/17" that is now near-complete). Verify memory against code
  before relying on it.

## Known debt (fix deliberately; do not imitate)

These violate Philosophy rule #1 and are tracked for removal:

1. `convert/mod.rs` (~line 2298): bibliography-heading detection string-matches
   `参考文献` / `REFERENCES` / `References` / `Bibliography`. Replace with
   `BibliographyElem`-driven detection.
2. `convert/recovery.rs` (~lines 83–85, 113): caption skipping string-matches
   `表 ` / `图 ` / `Table ` / `Figure `. Replace with `FigureElem` / caption
   supplement metadata.
