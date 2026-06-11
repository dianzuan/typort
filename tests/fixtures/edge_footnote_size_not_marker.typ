// Regression: the footnote-text size must be measured from the footnote BODY
// text, not the superscript reference/marker size. The body carries superscript
// markers (#super, ~6pt) plus footnote reference marks; the footnote entry text
// renders at ~9pt (Typst's default footnote shrink). The old detector took the
// global-minimum small size and so pinned FootnoteText to the ~6pt marker size.
// The fix measures the footnote runs themselves. Uses the embedded Libertinus
// font so the snapshot/size is CI-stable (no system font dependency).
#set text(font: "Libertinus Serif", size: 10.5pt)

This body paragraph carries superscript markers#super[abc] and more#super[xyz],
creating a small superscript-size bucket of several glyphs, plus a real footnote
reference.#footnote[This footnote entry contains a fair amount of running text,
so its rendered size registers a substantial glyph count — clearly larger than
the superscript markers in the body and smaller than the body text itself.]

Additional body text keeps the body size dominant overall, and here is a second
footnote#footnote[A second footnote entry, again with enough words that the
footnote body size is the dominant smaller-than-body running-text size.] too.
