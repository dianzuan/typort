// Test: list of figures and list of tables via outline
// Pandoc #1965/#10029: LOF/LOT field codes
// Pain point: outline(target: figure.where(...)) should map to TOC fields

#set heading(numbering: "1.")

#outline(title: "Contents")

= Introduction

Some text.

#figure(
  rect(width: 80pt, height: 40pt, fill: blue.lighten(80%)),
  caption: [A blue rectangle],
)

#figure(
  table(columns: 2, [A], [B], [C], [D]),
  caption: [A simple table],
)

= Methods

#figure(
  rect(width: 80pt, height: 40pt, fill: red.lighten(80%)),
  caption: [A red rectangle],
)
