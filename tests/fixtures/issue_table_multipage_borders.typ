// Test: table spanning multiple pages retains correct borders
// Typst #6072: table hlines repeated on page break
// Pain point: w:tblBorders top/bottom vs per-row tcBorders on page split

#set page(height: 10em)

#table(
  columns: (1fr, 1fr),
  stroke: 1pt,
  [Header A], [Header B],
  [Row 1 Col 1], [Row 1 Col 2],
  [Row 2 Col 1], [Row 2 Col 2],
  [Row 3 Col 1], [Row 3 Col 2],
  [Row 4 Col 1], [Row 4 Col 2],
  [Row 5 Col 1], [Row 5 Col 2],
  [Row 6 Col 1], [Row 6 Col 2],
)
