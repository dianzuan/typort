// Test: table borders correct on colspan/rowspan cells
// Typst #8184: internal borders behind merged cells
// Pain point: merged w:tc with gridSpan should have correct w:tcBorders

#table(
  columns: (3em, 3em, 3em, 3em),
  stroke: 1pt,
  table.cell(colspan: 2)[AB], [C], [D],
  [E], table.cell(colspan: 3)[FGH],
  [I], [J], [K], [L],
)
