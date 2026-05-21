// Issue: pandoc#6670, pandoc#6208 — Table cell content uses "Compact" style
// overriding any custom paragraph style. Text inside table cells should
// preserve its intended styling, not be forced into a generic compact style.

#set text(size: 11pt)

= Table Cell Content Styling

#table(
  columns: (1fr, 2fr),
  align: (center, left),
  [*Header 1*], [*Header 2*],
  [Normal text], [This cell has _italic_ and *bold* text],
  [Centered], [
    This cell has a paragraph with specific formatting.

    And a second paragraph inside the same cell.
  ],
  [Small], [#text(size: 9pt)[Smaller text in this cell] and normal text],
)
