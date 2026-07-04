// Inline math inside a list item must become OMML, not leak its MathML
// glyphs as literal text (the list path used to bypass the <math> skip).
#set text(font: "Libertinus Serif")

- item one with $x^2 + y$ inline
- plain item

+ numbered with $a/b$ fraction
