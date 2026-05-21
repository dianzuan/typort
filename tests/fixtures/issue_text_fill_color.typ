// Test: text fill/color preserved in output
// Typst #6956: text fill not exported in HTML, needs PagedDocument fallback
// Pain point: w:color on w:rPr should reflect fill colors

#set text(fill: red)

This entire paragraph is red.

#text(fill: blue)[This is blue] and this is back to red.

#text(fill: rgb("#00AA00"))[Green text with hex color.]
