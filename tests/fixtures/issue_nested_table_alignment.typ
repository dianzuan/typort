// Test: nested table alignment must not inherit parent cell alignment
// Pandoc #6984: inner table cells inherit outer cell centering
// Pain point: inner w:tbl must have its own w:jc, not inherit parent w:pPr

#table(
  columns: (1fr, 1fr),
  align(center)[
    #table(
      columns: 1,
      align(left)[Left-aligned inner cell],
    )
  ],
  [Normal right cell],
)
