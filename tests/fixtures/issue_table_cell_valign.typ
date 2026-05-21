// Test: table cell vertical alignment
// Pandoc #5460: table cells lack w:vAlign
// Pain point: w:tcPr should contain w:vAlign for non-default alignment

#table(
  columns: (1fr, 1fr, 1fr),
  rows: 4em,
  table.cell(align: horizon)[Middle],
  table.cell(align: bottom)[Bottom],
  [Top default],
)
