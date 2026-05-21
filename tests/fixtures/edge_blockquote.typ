// Test: block quotes produce left-indented paragraphs
// Edge case from Pandoc #11203: spurious symbols in quote output

#quote(block: true)[
  This is a block quote that should be indented from both margins.
]

Normal paragraph after the quote.

#quote(block: true)[
  A second block quote with *bold* and _italic_ formatting inside.
]

Final paragraph.
