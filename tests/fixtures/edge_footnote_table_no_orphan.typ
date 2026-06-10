// A footnote + a table (no `#line()`) must NOT leak the footnote body into the
// document body, nor invent a horizontal rule from the footnote separator or the
// table's own border lines. (Recovery over-scraped the page-bottom footnote zone
// and treated wide geometry lines as body rules.)

Body paragraph with a footnote#footnote[This footnote body must stay in the footnote zone, not the document body.] in it.

#table(
  columns: (1fr, 2fr),
  [Left cell], [Right cell content],
  [Another], [Row of data here],
)
