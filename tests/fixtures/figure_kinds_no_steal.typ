// A quote/text figure must not steal the raster of the drawing figure that
// follows it (rasters are keyed by figure location, not popped in order),
// and its body text must survive.
#set text(font: "Libertinus Serif")

#figure(
  [QUOTE-BODY-SENTENCE stays as editable text.],
  caption: [A quote figure],
)

#figure(
  box(width: 60pt, height: 40pt)[
    #place(top + left, curve(
      stroke: blue,
      curve.move((0pt, 30pt)),
      curve.cubic((20pt, 0pt), (40pt, 60pt), (60pt, 30pt)),
    ))
  ],
  caption: [A drawing figure],
)
