// Test: colbreak() should produce w:br type="column"
// Pandoc #8207: column breaks not supported, become page breaks
// Pain point: w:br type="column" instead of type="page"

#page(columns: 2)[
  First column content here.

  More first column text.

  #colbreak()

  Second column content here.

  More second column text.
]
