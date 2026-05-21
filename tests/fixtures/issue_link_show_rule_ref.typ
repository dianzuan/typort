// Test: link show rules should apply to @ref cross-references
// Typst #5614: show link rules not applied to @label references
// Pain point: REF field runs may lack color styling from link show rule

#set heading(numbering: "1.")
#show link: set text(fill: blue)

= Introduction <intro>

Some text here.

= Methods

See @intro for details. Also visit #link("https://example.com")[Example].
