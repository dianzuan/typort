// Test: deeply nested lists (3+ levels) and mixed ordered/unordered
// Pandoc #9994: mixed nested lists lose numbering

- Level 0 bullet
  - Level 1 bullet
    - Level 2 bullet
      - Level 3 bullet

+ First item
  + Sub-item one
    + Sub-sub-item
  + Sub-item two
+ Second item

- Bullet parent
  + Ordered child one
  + Ordered child two
    - Bullet grandchild
