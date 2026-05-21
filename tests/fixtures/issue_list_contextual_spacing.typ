// Test: list paragraphs should use contextual spacing
// Pandoc #7280: lists have extra inter-item spacing without w:contextualSpacing
// Pain point: list items should not have extra space between same-style paragraphs

- Item A
  - Sub-item 1
  - Sub-item 2
- Item B
- Item C

+ First
+ Second
  + Nested second
+ Third
