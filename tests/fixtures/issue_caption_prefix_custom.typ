// Test: custom figure caption supplement/prefix
// Pandoc #7451: caption prefix hard-coded as "Figure"/"Table"
// Pain point: localized or custom supplement text

#figure(
  table(columns: 2, [A], [B], [1], [2]),
  caption: [Sample data],
  supplement: [表],
)

#figure(
  rect(width: 4cm, height: 2cm, fill: gray.lighten(80%)),
  caption: [A diagram],
  supplement: [Fig.],
)
