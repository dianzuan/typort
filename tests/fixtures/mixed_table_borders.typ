// Table styling must be decided PER TABLE from the rules drawn inside that
// table's own introspection bracket. The footnote separator (a wide flat
// line outside any table) must not restyle anything; the borderless table
// must stay borderless; the three-line table keeps its own rule weights.
#set text(font: "Libertinus Serif")

A borderless table#footnote[The footnote separator is a wide flat line.]:

#table(stroke: none, columns: 2, [a-one], [b-one], [a-two], [b-two])

A three-line table:

#table(
  columns: 2,
  stroke: none,
  table.hline(stroke: 1pt),
  table.header([Head-A], [Head-B]),
  table.hline(stroke: 0.5pt),
  [x-one], [y-one],
  table.hline(stroke: 1pt),
)
