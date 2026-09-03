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
   (`BibliographyElem`, `FigureElem`, `FootnoteElem`, caption supplement metadata)
   or a declared source value (`#set text(lang: ...)`), not rendered text.
   `convert/bibliography.rs` is the model to copy.

2. **Detect, don't assume.** Styling (fonts, sizes, colors, spacing, alignment,
   margins) is read from the actual rendering or the source AST — never
   hardcoded for a document we happened to test with. Ask "does this work for
   *any* Typst document?", not "does this work for my fixture?".

3. **Semantic-first, geometry-as-fallback.** Prefer HTML semantics + the
   introspector (which preserve "this is a heading / footnote / equation"). Fall
   back to Paged geometry only for things HTML cannot express. Geometry inference
   is the lossy path we exist to avoid — keep it contained.

4. **Before declaring something impossible, check all three sources.** If Typst
   renders it correctly, the information exists *somewhere* — a "can't" almost
   always means "not in the representation I happened to look at." Elements consumed
   during compilation are queryable in neither the HtmlDocument nor the
   PagedDocument (introspector hits = 0), yet are still present in the **source AST**:
   `#colbreak()`, `smallcaps`, `#set text(lang:)`, and `datetime.today()`'s inputs
   are all recovered from source, not from the compiled output. Probe (compile +
   query/parse) before concluding, and look for a precedent — smallcaps' source-AST
   recovery was the template for colbreak. Say "I haven't found a way" rather than
   "there is no way" until you have proof there is no principled rule.

## Architecture in one paragraph

typort compiles the same source to **`HtmlDocument`** (semantics + document order
+ introspector) *and* **`PagedDocument`** (fonts, geometry, images, layout-only
content), and additionally **re-parses the source AST** for authoritative `set`
rules. HTML is the skeleton; Paged paints and patches it; the AST overrides both
when the author declared a value. This is **three** sources, not two — don't let
anyone "simplify" it back to a dual-compilation description. Full detail in
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
- **Argument threading:** shared walk state is bundled in context structs
  (`WalkCtx<'a>` for the HTML walk, `InlineFmt` for inline formatting flags,
  `TableWidthCtx` for table sizing) rather than positional parameters. The
  former `clippy.toml` `too-many-arguments-threshold` override was removed once
  that debt was repaid — new shared state goes into one of these structs, not a
  new positional parameter.
- **File size:** don't grow files reflexively. The HTML walk is split by
  responsibility under `convert/` (`block`, `inline_walk`, `headings`, `tables`,
  `lists`, `source`, `smallcaps`, `postprocess`, and `dom`); paged-style
  extraction is split under `convert/page/` (`units`, `style`, `source_ast`,
  `hanging_indent`, `reachable`, `sections`, `margin`, `run_style`, and
  `language`); recovery is split under `convert/recovery/` (`lines`,
  `deduplication`, `insertion`, `horizontal_rules`, and `table_rules`), with
  shared walk/recovery text normalisation in `convert/text_norm.rs`; and the
  OOXML writer is split by emitted part under `typort-ooxml/src/writer/`. Each
  `mod.rs` is a re-exporting facade. Put a new converter, paged-style
  responsibility, or writer part in its matching module rather than growing an
  entry module. Tests are already split this way:
  `crates/typort-cli/tests/integration/` is one file per test area (see the Testing
  section below) — add a new area module rather than growing an existing one.

## Testing

- Tests are **fixture-driven**: a `.typ` file under `tests/fixtures/` is converted
  and asserted on. Add a fixture for new features. Integration tests are split into
  `math`, `tables`, `structure`, `headings`, `formatting`, `images`, `recovery`,
  `lists`, `footnotes`, `misc`, `bibliography`, `fonts_cjk`, `golden`, and `visual`
  area modules, with fixture conversion shared through `tests/common`.
- **Any change under `convert/recovery/` or to the `convert/page/` heuristics must
  ship with a fixture-based regression test** for the specific case — that code is
  the most fragile part of the system (geometry → semantics inference with magic
  thresholds; see ARCHITECTURE.md "fragile seam").
- **Golden snapshots** (`crates/typort-cli/tests/integration/golden.rs`) pin the
  exact `word/document.xml` for a curated set of fixtures under `tests/snapshots/`,
  so output-formatting drift surfaces as a reviewable diff — the suite's only
  oracle for output *quality*, not just presence. After an intentional output
  change, regenerate and **review the diff before committing**:
  ```bash
  UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden
  git diff tests/snapshots
  ```
  The set is curated for CI-safety: only fixtures whose fonts are **embedded**
  (Libertinus) or **constant** ("Courier New") are snapshotted. **Declaring a CJK
  font in source is *not* enough** — it pins the font name in the output, but
  properties detected from rendering (bold weight, size) still need that font
  *installed*, and CI installs no CJK fonts (the World loads system fonts). So a
  CJK fixture flakes on CI even with `#set text(font: …)` — e.g. `complex_paper`
  declares "Noto Serif SC", which CI lacks, so its author-name bold detection
  diverged. **Don't byte-snapshot any CJK fixture**; cover it with substring tests.
- **Visual-regression tests** (`visual_regression_*`) render the docx via
  LibreOffice/pdftoppm/ImageMagick and RMSE-compare against Typst's own PDF. They
  are `#[ignore]`d (those tools aren't in CI) but **panic loudly on a missing
  tool** when opted into — they never silently pass. Run them with
  `cargo test -p typort -- --ignored`.

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

**Toolchain (pinned 2026-09-01):** `rust-toolchain.toml` pins the channel
(currently 1.98.0), so CI and local runs use the same compiler, clippy, and
rustfmt. Before the pin, `dtolnay/rust-toolchain@stable` floated to the newest
stable and twice turned a green tree red with no code change (2026-05-19,
2026-07-12: new rustfmt layout rules / new clippy lints applied to existing
code). Bump the pin deliberately, in a commit that also fixes whatever the new
toolchain flags. If your local rust is not rustup-managed (e.g. a distro
package), make sure it matches the pinned version before trusting a local green.

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

## Hardcoded-language cleanup (done — kept as precedent)

The P1 violations that prompted this rule have been removed. Recorded here so
the *approach* is reused, not re-introduced:

1. Bibliography hanging indent no longer string-matches `参考文献` / `References`
   in `apply_paragraph_formatting`; it is driven by the semantic
   `doc-bibliography` role during the HTML walk (a hand-written `= 参考文献`
   heading is just text to Typst, so typort treats it as text).
2. Caption skipping in `convert/recovery/deduplication.rs` no longer matches `表 ` /
   `图 ` / `Table ` / `Figure `; captions are deduplicated by the semantic text
   the figure path already emitted.
3. Document language is derived from `#set text(lang:, region:)`
   (`apply_language_override`), not guessed from CJK-glyph presence.

If you find a remaining `if text.contains("<some word>")` driving layout, it is
a regression of this rule — fix it the same way.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `dianzuan/typort` (via the `gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root (created lazily by `/domain-modeling`). See `docs/agents/domain.md`.
