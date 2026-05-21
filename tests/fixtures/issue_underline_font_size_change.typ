// Test: underline/strikethrough consistent across font size changes
// Typst #8145: text decoration breaks at font-size boundaries
// Pain point: w:u should apply uniformly across runs with different w:sz

#underline[Normal size #text(size: 0.7em)[smaller text] back to normal]

#strike[Regular #text(size: 1.5em)[bigger] regular again]

#overline[Mixed #text(size: 8pt)[tiny] and #text(size: 16pt)[large] text]
