// Test: block quotation inside footnote
// Pandoc #2128: block quotation inside footnote not rendered
// Pain point: quoted text appears as regular footnote text

Some text with a footnote containing a block quote.#footnote[
  This is footnote text.

  #quote(block: true)[
    This is a block quote inside the footnote.
  ]

  More footnote text after the quote.
]

Another paragraph.
