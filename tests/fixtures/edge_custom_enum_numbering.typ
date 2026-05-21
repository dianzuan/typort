// Test: custom enum numbering patterns (roman, alpha, nested)
// typst#905: more flexible separators in numbering patterns
// Pain point: Word needs specific numFmt values for each pattern

// Roman numeral numbering
#set enum(numbering: "I.")
+ First major point
+ Second major point
+ Third major point

// Alphabetical numbering
#set enum(numbering: "a)")
+ Alpha item
+ Beta item
+ Gamma item

// Nested with different patterns per level
#set enum(numbering: "1.a.i.")
+ Top level
  + Second level
    + Third level
  + Another second
+ Back to top
  + And nested again

// Parenthesized numbering (common in legal/academic)
#set enum(numbering: "(1)")
+ First clause
+ Second clause
+ Third clause
