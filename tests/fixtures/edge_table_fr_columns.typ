// Regression: fractional column tracks must yield proportional Word column
// widths, not three equal columns. `columns: (1fr, 2fr, 3fr)` is a 1:2:3 split
// of the available width, i.e. 16.6% : 33.3% : 50%. The OOXML writer previously
// fell back to equal distribution because `cell.width_pct` was never populated
// from the Typst column track sizing read off the `TableElem`.
//
// Asserted in crates/typort-cli/tests/integration.rs
// (fr_column_tracks_produce_proportional_widths): the three header cells must
// carry `w:tcW` percentages near 833 / 1666 / 2500 (fiftieths of a percent of
// the 5000 = 100% table width), NOT 1666 / 1666 / 1666.

#table(
  columns: (1fr, 2fr, 3fr),
  [Narrow], [Medium], [Wide],
  [d], [e], [f],
)
