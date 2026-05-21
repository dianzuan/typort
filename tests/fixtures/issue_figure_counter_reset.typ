// Test: figure counter reset mid-document
// typst-hs #75: counter isn't resetting properly for figures
// Pain point: caption numbering after counter.update(0)

#figure(
  table(
    columns: (1fr, 1fr),
    table.header([A], [B]),
    [Content], [Content],
  ),
  caption: [First table],
)

#counter(figure.where(kind: table)).update(0)

#figure(
  table(
    columns: (1fr, 1fr),
    table.header([C], [D]),
    [Content], [Content],
  ),
  caption: [Table after reset],
)
