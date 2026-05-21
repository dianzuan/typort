// Test: colored text via direct text() calls and show rules
// Pandoc #3338: text color dropped during conversion

This is #text(fill: red)[red text] and this is #text(fill: blue)[blue text].

#text(fill: rgb("#00AA00"))[Green text with hex color.]

Normal text after colored text.
