// Test: figure captions preserved with correct ordering
// Pandoc #11102: caption before figure, not centered, no numbering prefix
// Pandoc #11138: no numbered captions for tables

#figure(
  table(
    columns: 3,
    [Name], [Age], [City],
    [Alice], [30], [Beijing],
    [Bob], [25], [Shanghai],
  ),
  caption: [Demographics of participants],
)

Some text between figures.

#figure(
  table(
    columns: 2,
    [Item], [Price],
    [Apple], [\$1.00],
    [Banana], [\$0.50],
  ),
  caption: [Fruit prices at the market],
)
