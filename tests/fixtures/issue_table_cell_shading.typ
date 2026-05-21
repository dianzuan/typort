// Test: table cell background fill / shading
// Pandoc #2667: cell fill not mapped to w:shd
// Pain point: w:tcPr should have w:shd with fill color

#table(
  columns: (1fr, 1fr),
  table.cell(fill: yellow)[Yellow cell],
  table.cell(fill: blue.lighten(80%))[Light blue cell],
  [No fill], [No fill],
  table.cell(fill: rgb("#CCFFCC"))[Green cell],
  table.cell(fill: gray.lighten(60%))[Gray cell],
)
