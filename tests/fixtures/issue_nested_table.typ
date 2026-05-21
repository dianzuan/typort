// Test: table nested inside another table cell
// Pandoc #6984/#6983: nested tables inherit alignment / corrupt docx
// Pain point: nested w:tbl must not inherit parent w:jc

#table(
  columns: (1fr, 1fr),
  [
    #table(
      columns: (auto, auto),
      [Inner A], [Data],
      [Row 2], [More],
    )
  ],
  [Outer cell content],
)
