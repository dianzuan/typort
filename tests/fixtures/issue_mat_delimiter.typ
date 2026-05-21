// Test: matrix with custom delimiter preserved
// typst-hs #64: mat(delim: "[", ...) delimiter not respected
// Custom delimiters on matrices/vectors should appear in OMML

$ mat(delim: "[", 1, 0; 0, 1) $

$ vec(delim: "(", x, y, z) $

$ mat(delim: "|", a, b; c, d) $
