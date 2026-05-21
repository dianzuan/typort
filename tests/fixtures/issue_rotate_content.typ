// Test: content inside #rotate is not silently dropped
// Pandoc #11531: text inside #rotate block disappears
// Pandoc #11598: figure caption lost when rotated

This text is normal.

#rotate(0deg)[This text has zero rotation.]

This text follows rotated content.
