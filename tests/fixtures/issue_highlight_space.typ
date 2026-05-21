// Test: whitespace at element boundaries preserved
// typst#8100: trailing space inside #highlight lost in HTML
// Spaces between inline-formatted and plain text must not collapse

#highlight[Hello ]World

This has #strong[bold ]text after.

Mixed #emph[italic ]and normal text.
