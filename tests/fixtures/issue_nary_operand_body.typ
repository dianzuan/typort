// Regression: a big operator (∫, ∑) must carry its operand inside OMML's
// <m:e>, not leave it empty with the integrand as detached siblings (a spec
// violation that breaks round-trip/accessibility). The operand boundary is a
// Relation-class symbol (=, <, …) — Typst's own math classification — so the
// `= S` below stays OUTSIDE the summation's body.
#set page(width: 14cm, height: 8cm, margin: 1cm)

$ sum_(i=1)^n i^2 = S $
