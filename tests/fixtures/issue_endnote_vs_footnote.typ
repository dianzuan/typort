// Issue: pandoc#4041, pandoc#1425 — Footnotes vs endnotes differentiation
// Pandoc converts all endnotes to footnotes; Word supports both types.
// typort should preserve footnotes correctly (endnotes are a future extension).

= Document with Notes

This paragraph has a footnote#footnote[This is a regular footnote at the bottom of the page.].

Here is another sentence with a footnote#footnote[Second footnote with *bold* and _italic_ formatting.].

And a third note referencing a formula: $E = m c^2$#footnote[Einstein's famous equation from 1905.].

= Second Section

More text with a footnote#footnote[A footnote in a different section to test cross-page behavior.].
