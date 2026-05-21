// Test: bookmark start/end must be inside w:p, not siblings
// Pandoc #8825: bookmark tags outside paragraph break cross-references
// Pain point: REF field updates copy extra content when bookmarks wrap paragraph

#set heading(numbering: "1.")

= Introduction <intro>
Some introductory text.

= Methods
See @intro for details on the introduction.
