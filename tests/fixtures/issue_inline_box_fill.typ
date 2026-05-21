// Test: inline box with fill should produce w:shd or w:highlight
// Typst #5872: box(fill: ...) has no w:rPr shading
// Pain point: filled inline boxes should have visual background in Word

Inline #box(fill: luma(90%), inset: 4pt)[highlighted box] in text.

Also #box(fill: yellow, inset: 2pt)[yellow inline] content.

And #highlight[native highlight] for comparison.
