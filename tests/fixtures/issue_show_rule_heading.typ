// Test: show rules on headings do not lose content
// Pandoc #11017: show rules applying text functions to headings fail
// typst-hs: show rules applied incorrectly or crash parser

#show heading.where(level: 1): set text(size: 18pt)
#show heading.where(level: 2): set text(fill: blue)

= Main Title

Some body text.

== Blue Subtitle

More body text.

=== Third Level

Final paragraph.
