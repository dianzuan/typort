// Test: custom heading sizes via show rules
// Expected: heading sizes detected from rendered output, not default 1.4/1.2 ratios

#set text(size: 10.5pt)
#show heading.where(level: 1): set text(size: 18pt)
#show heading.where(level: 2): set text(size: 15pt)
#show heading.where(level: 3): set text(size: 12pt)

= Custom Heading One

Paragraph under heading one.

== Custom Heading Two

Paragraph under heading two.

=== Custom Heading Three

Paragraph under heading three.

Normal body text here for comparison.
