// Regression fixture: inline math inside table cells. Typst's HTML export drops
// every equation, so each cell's math sits as an `equation` Tag sibling between
// the <p> text fragments. convert_cell_paragraphs only consumed the <p>s, so the
// math vanished and mixed text+math cells were split into stacked paragraphs.
#let three_line(cols, ..cells) = table(
  columns: cols,
  stroke: none,
  table.hline(stroke: 1.5pt),
  table.header(..cells.pos().slice(0, cols)),
  table.hline(stroke: 0.5pt),
  ..cells.pos().slice(cols),
  table.hline(stroke: 1.5pt),
)

#figure(
  three_line(
    2,
    [*符号*], [*含义*],
    [$bold(e)_1$], [经济生产性],
    [$M$分布 $times$ $v^*(M)$], [阈值函数],
  ),
  caption: [单元格内含行内公式的三线表],
)
