// Test: table caption with cross-reference bookmark
// Pandoc #7451/#9151: table captions need SEQ field + bookmark for REF
// Pain point: cross-referencing a labeled figure/table must resolve

#figure(
  table(columns: 2, [A], [B], [C], [D]),
  caption: [Sample data table],
) <tab:sample>

Some text between.

#figure(
  table(columns: 3, [X], [Y], [Z], [1], [2], [3]),
  caption: [Another table],
) <tab:another>

See @tab:sample and @tab:another for the data.
