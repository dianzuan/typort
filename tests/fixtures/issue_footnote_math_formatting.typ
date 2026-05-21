// Test: footnotes containing math and rich formatting
// Pandoc #4041: footnote content should preserve formatting
// Pain point: math and bold/italic inside footnotes

This has a math footnote#footnote[The formula $x^2 + y^2 = z^2$ is well-known.].

Another with formatting#footnote[Contains *bold* and _italic_ text.].

A third footnote#footnote[Multiple paragraphs:

Second paragraph in the footnote.]
