// Test: RTL table needs w:bidiVisual
// Pandoc #7695: tables in RTL documents appear column-reversed
// Pain point: w:tblPr should contain w:bidiVisual when text dir is RTL

#set text(dir: rtl, lang: "ar")

#table(
  columns: 3,
  [العمود الأول], [العمود الثاني], [العمود الثالث],
  [أ], [ب], [ج],
  [1], [2], [3],
)
