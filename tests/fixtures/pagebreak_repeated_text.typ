// The paragraph before this #pagebreak() also occurs earlier in the document.
// Text-anchored recovery snapped the break to the FIRST occurrence; span
// anchoring must keep it after the second.
#set text(font: "Libertinus Serif")

Opening section.

The same closing line.

Middle section.

The same closing line.

#pagebreak()

Final section.
