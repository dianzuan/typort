// Faithful (non-hardcoded) table conversion: cell alignment the author set in
// Typst must survive into Word, sourced from the semantic TableElem (the HTML
// export drops it). Vertical alignment -> <w:vAlign> in the cell's tcPr;
// horizontal alignment -> <w:jc> in the cell's paragraphs (Pandoc-style). A cell
// with no explicit alignment must get neither (don't override Word's defaults).
#set text(font: "Libertinus Serif", size: 10.5pt)

#table(
  columns: (1fr, 1fr),
  // A vertically-merged, vertically-centred cell (the "control variable" case).
  table.cell(rowspan: 2, align: horizon)[Merged],
  // A right-aligned cell (horizontal).
  table.cell(align: right)[R1],
  // A plain cell: no alignment override -> emits no jc / vAlign.
  [R2],
)
