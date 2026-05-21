// Test: color primitives and color methods
// typst-hs #35/#21: color.hsl/gradient/mix crash Pandoc parser
// Pain point: valid Typst color expressions must not crash converter

#text(fill: rgb("#ff5733"))[RGB colored text]

#text(fill: blue.lighten(40%))[Lightened blue]

#text(fill: eastern)[Named color: eastern]

Normal text after colored text.
