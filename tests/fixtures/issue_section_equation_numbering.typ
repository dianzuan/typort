// Test: section-dependent equation numbering
// typst#2652/#1896: equations numbered per chapter
// Pain point: compound numbering (2.3) in cross-references

#set heading(numbering: "1.")
#show heading.where(level: 1): it => {
  counter(math.equation).update(0)
  it
}
#set math.equation(numbering: n => {
  let h = counter(heading).get().first()
  "(" + str(h) + "." + str(n) + ")"
})

= Introduction
$ E = m c^2 $ <eq:einstein>
$ F = m a $ <eq:newton>

= Methods
$ integral_0^infinity e^(-x) dif x = 1 $ <eq:integral>

See @eq:einstein and @eq:integral.
