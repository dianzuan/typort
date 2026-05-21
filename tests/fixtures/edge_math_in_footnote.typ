// Test: math inside footnotes
// Pandoc #2881: math in footnotes not detected, namespace missing

This text has a footnote with math#footnote[The formula is $E = m c^2$ which relates energy and mass.].

Another paragraph with a display-math footnote#footnote[Consider the integral: $ integral_0^1 x^2 dif x = 1/3 $].
