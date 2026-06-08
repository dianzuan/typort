// A vector-drawing figure (Bézier curve + an inner text label) must be
// rasterized to a single image, with its label baked into the pixels rather
// than leaked into the body as a stray paragraph — even though the label is
// absent from the HTML and would otherwise be recovered from the paged output.
// A sibling table figure must stay an editable table, and both captions stay.
#set page(width: 12cm, height: 16cm, margin: 1cm)
#set text(font: "Times New Roman", size: 10.5pt)

Body text before the figures.

#figure(
  box(width: 5cm, height: 3cm)[
    #place(top + left, curve(
      curve.move((0pt, 40pt)),
      curve.cubic((20pt, 0pt), (40pt, 80pt), (80pt, 40pt)),
    ))
    #place(center, text(size: 9pt)[ZZLABELZZ])
  ],
  caption: [Drawn figure],
) <fig-draw>

#figure(
  table(columns: 2, [a], [b], [c], [d]),
  caption: [Real table],
) <tab-real>

Body text after the figures.
