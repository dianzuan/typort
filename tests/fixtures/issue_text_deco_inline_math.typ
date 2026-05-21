// Test: text decorations across inline math boundaries
// typst#2200: underline/highlight/strike don't propagate into math
// Pain point: decorations on OMML runs

This has #underline[underlined math $a^2 + b^2 = c^2$ inside].

Also #highlight[highlighted $E = m c^2$ formula].

And #strike[struck-through $x + y$ expression].
