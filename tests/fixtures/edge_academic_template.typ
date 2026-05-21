// Test: realistic academic template with numbered headings, appendix,
// abstract environment, keywords, and author affiliations
// Pain point: show rules that reformat headings + appendix counter reset
// break many converters

#set page(margin: (top: 2.5cm, bottom: 2.5cm, left: 2.5cm, right: 2.5cm))
#set text(size: 12pt)
#set par(first-line-indent: 2em, justify: true)
#set heading(numbering: "1.1")

// Suppress indent after headings
#show heading: it => {
  it
  par(text(size: 0pt, ""))
}

#align(center)[
  #text(size: 16pt, weight: "bold")[A Study on Convergence Properties]

  #v(0.5em)
  Author One#super[1], Author Two#super[2]

  #text(size: 10pt)[
    #super[1]Department of Mathematics, University A \
    #super[2]Institute for Computing, University B
  ]
]

#v(1em)

#block(inset: (left: 2em, right: 2em))[
  *Abstract.* We prove that every bounded sequence in a complete metric space
  has a convergent subsequence. Our proof simplifies the classical argument
  by leveraging a new compactness criterion.

  *Keywords:* convergence, compactness, metric spaces, subsequences
]

#v(1em)

= Introduction

The study of convergence is fundamental to analysis. In this paper we
consider sequences in metric spaces $(X, d)$.

= Main Results

== Definitions

Let $(X, d)$ be a metric space. A sequence $(x_n)$ is _Cauchy_ if
for every $epsilon > 0$ there exists $N$ such that $d(x_m, x_n) < epsilon$
for all $m, n >= N$.

== Theorem

$ forall epsilon > 0, exists N in NN : m, n >= N ==> d(x_m, x_n) < epsilon $

= Conclusion

We have shown the desired result.

// Appendix with reset numbering
#set heading(numbering: "A.1")
#counter(heading).update(0)

= Supplementary Proofs

== Proof of Lemma 1

The detailed proof is omitted for brevity.

== Additional Data

See the supplementary materials.
