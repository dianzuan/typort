// A deliberate per-run font override on an all-digit run must be PRESERVED.
// (Regression: an over-broad "any non-letter run with a non-baseline font is a
// fallback artifact" rule used to drop this; only a true OpenType MATH-table
// fallback should be normalized.) DejaVu Sans Mono is embedded by typst-kit, so
// this reproduces in CI.

Normal body text and #text(font: "DejaVu Sans Mono")[12345] then back to normal.
