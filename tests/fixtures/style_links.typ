// Test: hyperlinks with no special color (Typst default)
// Expected: no hardcoded link color, underline only

#set text(size: 11pt)

Visit #link("https://typst.app")[the Typst website] for more information.

Here is another #link("https://example.com")[example link] in a paragraph.

Normal text continues here.
