// Test: numbered captions for table figures
// Pandoc #11138: numbered captions missing for tables in docx
// Pain point: "Table N:" prefix lost during conversion

#figure(
  table(
    columns: (1fr, 1fr),
    table.header([A], [B]),
    [Content], [Content],
  ),
  caption: [First table with a caption],
)

#figure(
  table(
    columns: (1fr, 1fr),
    table.header([C], [D]),
    [More], [Data],
  ),
  caption: [Second table with a caption],
)
