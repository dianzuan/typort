// Regression: footer page numbers must NOT leak into body text.
#set page(numbering: "i")
#set text(font: "Times New Roman", size: 12pt)

= Alpha
#lorem(120)
#pagebreak()
= Beta
#lorem(120)
#pagebreak()
= Gamma
#lorem(120)
#pagebreak()
= Delta
#lorem(120)
