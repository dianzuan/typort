// Test: mixed ordered/unordered nested lists preserve numbering
// Pandoc #9994/#7280: mixed nested lists lose numbering
// Pain point: numId/ilvl chain breaks when list type changes

+ First ordered item
  - Bullet sub-item A
  - Bullet sub-item B
+ Second ordered item
  - Another bullet
+ Third ordered item
