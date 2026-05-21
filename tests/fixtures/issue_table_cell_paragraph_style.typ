// Test: table cell paragraph style vs body paragraph style
// Pandoc #5460/#6208: table cells use wrong paragraph style
// Pain point: table cells should not have first-line indent

#set par(first-line-indent: 2em, spacing: 1.5em)

This is a normal paragraph with first-line indent and paragraph spacing.

- List item one
- List item two

#table(
  columns: (1fr, 1fr),
  [Table cell content], [Should have tight spacing],
  [Another row], [With different style],
)

Another paragraph after the table.
