// Regression: a three-line (三线表) table — stroke:none plus explicit hlines
// (thick top, thin under-header, thick bottom; no verticals, no inner rules) —
// was emitted as a full boxed grid. Borders are now read from the rules actually
// drawn in the PagedDocument: no vertical lines means three-line, so verticals and
// inner-row rules are suppressed and only the top/bottom rules and a header
// separator are kept.
#table(
  columns: (auto, 1fr, 1fr),
  stroke: none,
  table.hline(stroke: 1.5pt),
  table.header([*列一*], [*列二*], [*列三*]),
  table.hline(stroke: 0.5pt),
  [a1], [b1], [c1],
  [a2], [b2], [c2],
  table.hline(stroke: 1.5pt),
)

正文段落。
