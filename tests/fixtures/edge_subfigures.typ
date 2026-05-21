// Test: subfigures arranged in a grid inside a figure
// typst#246: subfigures (57+ comments, very common request)
// Pain point: grid-of-images inside figure — converters often lose
// the caption or flatten the layout

#figure(
  grid(
    columns: 2,
    gutter: 1em,
    [#align(center)[_Subfigure placeholder (a)_]],
    [#align(center)[_Subfigure placeholder (b)_]],
  ),
  caption: [Comparison of two methods: (a) baseline, (b) proposed.],
) <fig:comparison>

As shown in @fig:comparison, the proposed method outperforms the baseline.

// Side-by-side tables (another common subfigure-like pattern)
#figure(
  grid(
    columns: 2,
    gutter: 2em,
    table(columns: 2, [A], [B], [1], [2]),
    table(columns: 2, [C], [D], [3], [4]),
  ),
  caption: [Left: training data. Right: test data.],
)
