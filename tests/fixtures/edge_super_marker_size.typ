// A superscript affiliation marker `#super[1]` renders small; its size must NOT
// leak (via same-text matching) onto a body "[1]" reference marker, which should
// stay at body size. (The reference [1] is literal text in markup — square
// brackets are only content blocks after a `#`.)

#set text(font: ("Libertinus Serif",), size: 10.5pt)

First Author#super[1] and Second Author#super[2] in the byline above.

#v(1em)

[1] A first reference entry whose leading marker must stay at body size here.

[2] A second reference entry, also kept at the body text size for its marker.
