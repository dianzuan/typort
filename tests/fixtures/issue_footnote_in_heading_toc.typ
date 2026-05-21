// Test: footnote in heading should not duplicate in TOC
// Typst #1880: heading footnote appears in TOC entry
// Pain point: TOC field should strip footnote ref from heading text

#outline()

= Introduction #footnote[A background note]

Some content here.

= Methods #footnote[See supplementary materials]

More content.

= Results

Final content.
