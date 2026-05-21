// Test: nested list indentation at each level
// Pandoc #6586: nested list indent hard-coded, misaligned
// Pain point: each level should increase indent proportionally

+ Level one item A
  + Level two item A.1
    + Level three item A.1.a
    + Level three item A.1.b
  + Level two item A.2
+ Level one item B

- Bullet level one
  - Bullet level two
    - Bullet level three
      - Bullet level four
