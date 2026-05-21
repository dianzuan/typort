// Test: text tracking (letter spacing) in w:rPr
// Typst #1125: tracking has no docx w:spacing equivalent emitted
// Pain point: w:rPr should contain w:spacing for tracked text

#text(tracking: 0.2em)[Wide tracked text] and normal text.

#text(tracking: -0.05em)[Tight tracked text] for comparison.
