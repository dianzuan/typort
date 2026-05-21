// Test: mixed portrait/landscape page orientation
// typst#4158: page.margin not affected by page.flipped
// Pain point: section breaks in Word must carry orient="landscape"
// and swap width/height in pgSz

#set text(size: 11pt)

= Portrait Section

This is normal portrait content on an A4 page.

Some body text to fill the first section.

#pagebreak()
#set page(flipped: true)

= Landscape Section

This section uses landscape orientation, commonly needed for
wide tables or figures in academic papers.

#table(
  columns: 6,
  [Col 1], [Col 2], [Col 3], [Col 4], [Col 5], [Col 6],
  [Data], [Data], [Data], [Data], [Data], [Data],
  [More], [More], [More], [More], [More], [More],
)

#pagebreak()
#set page(flipped: false)

= Back to Portrait

Final section returns to portrait orientation.
