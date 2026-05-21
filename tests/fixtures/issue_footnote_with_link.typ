// Test: footnotes containing links and cross-references
// tinymist #2296: footnotes and internal links fail in export
// Pain point: footnote with link/cross-ref interaction

#set heading(numbering: "1.")

= Introduction <intro>

Some text with a footnote#footnote[See the conclusion for details.].

Another footnote with a link#footnote[Visit a website for more.].

= Conclusion <conclusion>

This concludes the paper. Refer back to @intro.
