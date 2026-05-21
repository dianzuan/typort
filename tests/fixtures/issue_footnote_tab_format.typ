// Test: footnote text should have tab after reference mark
// Pandoc #2621: footnote starts with space instead of w:tab
// Pain point: footnotes.xml should have w:tab after w:footnoteRef

First sentence with a footnote.#footnote[This is the first footnote content.]

Second sentence.#footnote[This is the second footnote with more text to verify formatting.]

Third sentence.#footnote[Short note.]
