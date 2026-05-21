// Test: place() absolute positioning graceful handling
// Typst #5233/#6821: place() has no Word anchoring equivalent
// Pain point: placed content should not be lost, even without exact positioning

Some body text here.

#place(top + right)[TOP RIGHT]

More text below the placed content.

#place(bottom + center)[BOTTOM CENTER]

Final paragraph.
