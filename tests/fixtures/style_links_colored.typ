// Test: hyperlinks with custom show-rule color
// Expected: link color detected from rendered output

#set text(size: 11pt)
#show link: set text(fill: rgb("#0000FF"))

Visit #link("https://typst.app")[the Typst website] for more information.

Here is another #link("https://example.com")[example link] in a paragraph.

Normal text continues here.
