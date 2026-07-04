// A cell that contains a nested table serializes its `content` vector, not
// the legacy `paragraphs` clones. Paged style patches (here: a text color,
// detectable only from rendering) must land on the serialized copy.
#set text(font: "Libertinus Serif")

#table(
  columns: 1,
  [
    #table(columns: 1, [nested-inner-cell])
    Outer prose with #text(fill: red)[RED-NESTED-MARKER] inside.
  ],
)
