// A plain heading's runs should NOT repeat the Heading style's own bold + size
// (the pStyle already supplies them, and duplicating them fights a Word
// template), but a genuinely-distinct inline span inside a heading (here italic)
// must keep its override.

= Plain Heading One

Body text long enough to make the body size the dominant size in the document,
so detection picks it as the baseline and the heading size as a true heading
size, mirroring how real documents are laid out on the page.

== Heading With #emph[Italic] Span

More body text here, again deliberately long so the body baseline dominates the
size-frequency map used for style detection during conversion of the document.
