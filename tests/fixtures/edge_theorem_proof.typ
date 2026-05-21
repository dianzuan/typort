// Test: theorem/proof/definition environments (common academic pattern)
// No native Typst element — typically built with show rules + counters
// This is the most common pattern in math/CS papers

#let theorem-counter = counter("theorem")

#let theorem(body, name: none) = {
  theorem-counter.step()
  block(width: 100%, inset: 8pt, stroke: (left: 2pt + black))[
    *Theorem #context theorem-counter.display("1")#if name != none [ (#name)].*
    #emph(body)
  ]
}

#let proof(body) = {
  block(width: 100%)[
    _Proof._
    #body
    #h(1fr) $square$
  ]
}

#let definition(body, term: none) = {
  block(width: 100%, inset: 8pt, fill: luma(245))[
    *Definition#if term != none [ (#term)].*
    #body
  ]
}

= Preliminaries

#definition(term: "Continuity")[
  A function $f: RR -> RR$ is continuous at $x_0$ if
  $ lim_(x -> x_0) f(x) = f(x_0). $
]

#theorem(name: "Intermediate Value")[
  Let $f$ be continuous on $[a, b]$ with $f(a) < 0 < f(b)$.
  Then there exists $c in (a, b)$ such that $f(c) = 0$.
]

#proof[
  Suppose not. Then $f(x) != 0$ for all $x in (a, b)$.
  This contradicts the continuity of $f$. $bot$
]

#theorem[
  Every bounded monotone sequence in $RR$ converges.
]
