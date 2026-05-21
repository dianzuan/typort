// Test: footnotes — size should be 0.86em of body
// Expected: footnote font size detected from rendered output

#set text(size: 12pt)

This is body text at 12pt with a footnote#footnote[This footnote text should be approximately 10.3pt (0.86 × 12).].

Another paragraph with another footnote#footnote[Second footnote for additional testing.].

And a third paragraph without footnotes.
