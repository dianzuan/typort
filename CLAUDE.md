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
- `typst-kit` 0.14.2 — Font discovery helpers (embedded fonts only, no system fonts)
- `quick-xml` 0.37 — XML serialization (we do NOT use docx-rs)
- `zip` 2.x — .docx ZIP packaging

## Conventions

- Rust 2024 edition
- `#![warn(clippy::pedantic)]` in all crate roots
- Tests go in-module for unit tests, `crates/typort-cli/tests/` for integration tests
- Test fixtures in `tests/fixtures/`
- No docx-rs — all OOXML XML is generated via quick-xml for full control
- Typst World uses embedded fonts only (no system font dependency for reproducibility)
