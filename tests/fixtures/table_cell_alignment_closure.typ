#set text(font: "Libertinus Serif", size: 10pt)

#table(
  columns: 2,
  align: (x, _y) => if x == 0 { left } else { right },
  [Closure left],
  [Closure right],
)
