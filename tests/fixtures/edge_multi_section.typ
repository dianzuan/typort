// Test: multiple section breaks with different page settings
// Pandoc #10577: page settings only apply to last section

#set text(size: 11pt)

= Section One (A4 Portrait)

Content in the first section with default A4 portrait layout.

#pagebreak()
#set page(width: 279.4mm, height: 215.9mm)

= Section Two (US Letter Landscape)

Content in the second section with landscape layout.
This section should have different page dimensions.

#pagebreak()
#set page(width: 210mm, height: 297mm)

= Section Three (Back to A4)

Content in the third section, back to A4 portrait.
