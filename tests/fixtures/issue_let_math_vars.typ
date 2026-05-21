// Test: #let-bound variables interpolated in math
// typst-hs #34/#51: let variables with = in math mode fail
// Pain point: variable interpolation in math equations

#let a = $1$
#let b = $2$

$#a = #b$

#let lhs = $x + y$
#let rhs = $z$

$#lhs = #rhs$
