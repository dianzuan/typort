// Test show rule styling recovery from PagedDocument
#set text(font: "Linux Libertine", size: 10.5pt)

// Show rule: headings use a different font and are centered
#show heading.where(level: 1): set align(center)
#show heading.where(level: 1): set text(font: "DejaVu Sans", size: 18pt)

// Show rule: emphasis uses a specific color
#show emph: set text(fill: rgb("#0000FF"))

// Show rule: strong uses larger size
#show strong: set text(size: 14pt)

= Styled Heading

This is normal body text at 10.5pt.

This has *enlarged bold* in it.

This has _blue italic_ in it.
