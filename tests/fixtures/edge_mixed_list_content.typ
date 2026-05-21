// Test: lists containing mixed block-level content
// typst#3206: paragraph should contain tight lists and block equations
// typst#8308: spurious <p> around text with sublist in tight list
// Pain point: list items with math, tables, or multi-paragraph content

// List item containing a block equation
- The quadratic formula is:
  $ x = (-b plus.minus sqrt(b^2 - 4 a c)) / (2 a) $
- Next item after block math

// Numbered list with multi-paragraph items
+ First item with one paragraph.

  This is a continuation paragraph in the same item.

+ Second item is simpler.

// List item containing a table
- Summary of results:
  #table(columns: 2,
    [Method], [Score],
    [A], [92%],
    [B], [88%],
  )
- Next item after table

// Deeply nested mixed lists
+ Outer numbered
  - Inner bullet
    + Deep numbered
      - Deepest bullet
  - Another inner bullet
+ Back to outer

// List with inline code and math mixed
- Compute `f(x)` where $f(x) = x^2$
- Set `alpha = 0.05` so $alpha = 0.05$
- The result is `42`
