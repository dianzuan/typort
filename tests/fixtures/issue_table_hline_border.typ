// Test: custom table hline borders
// Pandoc #5460: table header row border hardcoded override
// Pain point: converter should respect source border intent, not hardcode

#table(
  columns: (1fr, 1fr),
  table.header([Column A], [Column B]),
  table.hline(stroke: 2pt),
  [Data 1], [Data 2],
  [Data 3], [Data 4],
  table.hline(stroke: 0.5pt),
)
