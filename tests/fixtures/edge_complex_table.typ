// Test: complex table with colspan + rowspan combined
// Pandoc #6983: nested/complex merges produce unreadable Word content

#table(
  columns: 4,
  table.cell(colspan: 2)[Header A-B], table.cell(colspan: 2)[Header C-D],
  table.cell(rowspan: 2)[Row 1-2, Col A], [B1], [C1], table.cell(rowspan: 2)[Row 1-2, Col D],
  [B2], [C2],
  table.cell(colspan: 4)[Full width footer row],
)
