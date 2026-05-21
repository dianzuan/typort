// Test: document ending in landscape has no trailing blank page
// Pandoc #7255: final w:sectPr mismatch creates trailing blank page
// Pain point: last sectPr must match actual final page layout

#set page(width: 210mm, height: 297mm)

= Portrait Section

Some content in portrait.

#pagebreak()
#set page(width: 297mm, height: 210mm)

= Landscape Section

This is the final section in landscape. No trailing blank page expected.

#table(
  columns: 5,
  ..range(15).map(i => [Cell #str(i)])
)
