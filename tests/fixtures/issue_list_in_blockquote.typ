// Test: lists inside blockquotes must retain indentation
// Pandoc #6894: numbered lists in blockquotes are flush-left
// Pain point: w:ind on list paragraphs inside blockquote should add base indent

#quote(block: true)[
  + First ordered item
  + Second ordered item
  + Third ordered item
]

Some normal text between.

#quote(block: true)[
  - Bullet one
  - Bullet two
    - Nested bullet
]
