// Test: dashed/dotted table border styles
// Pandoc #5460: border style not mapped, always "single"
// Pain point: w:top/w:bottom val should reflect dash pattern

#table(
  columns: (1fr, 1fr),
  stroke: (dash: "dashed", thickness: 1pt),
  [A], [B],
  [C], [D],
)

#v(1em)

#table(
  columns: (1fr, 1fr),
  stroke: (dash: "dotted", thickness: 1pt),
  [E], [F],
  [G], [H],
)
