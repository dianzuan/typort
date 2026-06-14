// A forced line break (`\`) inside a paragraph must become a real <w:br/>, not be
// dropped (which glues the surrounding words together).
#set text(font: "Libertinus Serif", size: 10.5pt)
First line \
Second line in the same paragraph.
