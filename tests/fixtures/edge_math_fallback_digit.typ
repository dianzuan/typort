// Universal repro (no genre/keyword logic) for per-glyph math-font fallback.
//
// A non-math body font with a CJK fallback (the dual-font path that triggers
// Typst's per-glyph fallback) plus a real equation so a math face is loaded
// into layout and available to fall back onto. Typst's layout shapes the
// isolated bracketed digit "[7]" with the math variant of the body face (a face
// carrying an OpenType MATH table); copying that paged run style verbatim used
// to leak `w:rFonts w:ascii="...Math"` onto the plain digit. The double space
// also exercises the whitespace-run case.

#set text(font: ("New Computer Modern", "Noto Serif SC"), size: 10.5pt)

正文文字 reference [7] and more  text. 见文献 [12]。

行内公式 $x = 7$ 之后的数字 123。
