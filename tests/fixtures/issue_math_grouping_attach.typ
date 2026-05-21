// Test: various sub/superscript attachment patterns
// typst#8028: no explicit grouping-only parentheses in math
// Pain point: different sub/sup attachment positioning

$ f^a_b $

$ f_b^a $

$ attach(f, t: a, b: b) $

$ attach(f, tl: a, br: b) $
