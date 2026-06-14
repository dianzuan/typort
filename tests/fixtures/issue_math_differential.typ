// Regression: the differential `d` of `dif` must survive into the OMML.
// typst 0.15 wraps `dif` in a ClassElem(Unary, upright d) that 0.14 emitted
// bare; the OMML converter must descend ClassElem or the `d` is dropped,
// turning `integral y dif x` into `integral y x` (a math corruption).
$ integral y dif x = z $
