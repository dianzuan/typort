// Issue: pandoc#5460 — Hard-coded table header row border overrides table style
// The docx writer should not hard-code a bottom border on header rows,
// as this overrides any table style the user has defined.
// typort should only emit borders that are explicitly set in the source.

= Table Border Control

#table(
  columns: (1fr, 1fr, 1fr),
  stroke: none,
  table.header(
    [*Col A*], [*Col B*], [*Col C*],
  ),
  [Data 1], [Data 2], [Data 3],
  [Data 4], [Data 5], [Data 6],
)

#table(
  columns: (1fr, 1fr),
  stroke: 0.5pt,
  table.header(
    [*Name*], [*Value*],
  ),
  [Alpha], [100],
  [Beta], [200],
  [Gamma], [300],
)

#table(
  columns: (auto, auto),
  stroke: (x: none, y: 0.5pt),
  table.header(
    [*Key*], [*Description*],
  ),
  [A], [Only horizontal borders],
  [B], [No vertical borders],
)
