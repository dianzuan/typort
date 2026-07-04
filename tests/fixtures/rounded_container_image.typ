// An image inside a rounded (curve-bearing) container must still be embedded:
// image content now comes from the <img> src data-URL, so the container's
// curves cannot knock the image out of a positional queue.
#set text(font: "Libertinus Serif")

Before.

#block(fill: luma(230), radius: 4pt, inset: 4pt)[#image("tiny.gif")]

#figure(image("tiny.svg"), caption: [A real figure])

After.
