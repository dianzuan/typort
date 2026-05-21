// Test: block quote with attribution
// Pandoc #11203: block quote with attribution produces spurious characters
// Pain point: attribution formatting must be correct

#quote(block: true)[
  To be, or not to be, that is the question.
]

#quote(block: true, attribution: [William Shakespeare])[
  All that glitters is not gold.
]

Some text between quotes.

#quote(block: true, attribution: [Albert Einstein])[
  Imagination is more important than knowledge.
]
