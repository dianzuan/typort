// Test: text decorations propagating across inline math boundaries
// typst#8153: inconsistent propagation of markup decorators into math mode
// typst#1043: underline doesn't apply to inline math
// Pain point: bold/italic propagate but underline/highlight do not

// Bold propagates into math
*The equation $x^2 + y^2 = r^2$ is bold.*

// Italic propagates into math
_The formula $a + b = c$ is italic._

// Underline around math — may not propagate in Typst
#underline[The value $alpha = 0.05$ is significant.]

// Highlight around math — does not cover math span
#highlight[Reject $H_0$ when $p < alpha$.]

// Strike through math
#strike[Obsolete formula: $E = m c$]

// Mixed: bold + underline + math
*#underline[Important: $f(x) = x^2$]*
