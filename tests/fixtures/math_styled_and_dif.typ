// Regression fixture for math style wrappers (bold/bb/cal) and the upright
// differential operator `dif`. Before the StyledElem fix these all vanished
// silently, leaving empty <m:e> bases (see typort-math convert_content).
#let dif = math.upright("d")

Blackboard bold and calligraphic: $M_i in bb(R)_+$ and $cal(H) = bb(R)^n$.

Bold vectors: $bold(e)_1$, $bold(s)^*$, $bold(h)_(i,k)$.

Differential operator: $integral M dif G(M)$ and the derivative $frac(dif v, dif M)$.
