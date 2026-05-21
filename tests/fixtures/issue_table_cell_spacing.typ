// Test: table cell paragraphs with tight spacing
// Pandoc #2667: multi-paragraph cells inherit document spacing
// Pain point: table cells should use tight spacing

#set par(spacing: 1.5em)

#table(
  columns: (1fr, 1fr),
  [*Fruit*], [*Advantages*],
  [Bananas],
  [
    Built-in wrapper.

    Bright color.
  ],
  [Oranges],
  [
    Cures scurvy.

    Tasty flavor.
  ],
)
