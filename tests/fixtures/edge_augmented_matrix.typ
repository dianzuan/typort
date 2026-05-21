// Test: augmented matrix and special math constructs
// typst#8253: augmented matrix support in MathML export
// Pain point: mat(augment:) has no direct OMML equivalent —
// must approximate with a vertical bar column or split matrices

// Augmented matrix (linear system)
$ mat(
  1, 0, 3;
  0, 1, -2;
  augment: #2,
) $

// Row reduction example
$ mat(
  1, 2, 3, 4;
  0, 1, 2, 3;
  0, 0, 1, 1;
  augment: #3,
) $

// Piecewise function via cases (already supported, but test alongside)
$ f(x) = cases(
  x^2 & "if" x >= 0,
  -x & "if" x < 0,
) $

// Block matrix (partitioned)
$ mat(
  A, B;
  C, D;
) = mat(
  1, 0, 0, 0;
  0, 1, 0, 0;
  0, 0, 1, 0;
  0, 0, 0, 1;
) $

// Vector with labels
$ vec(x_1, x_2, dots.v, x_n) $
