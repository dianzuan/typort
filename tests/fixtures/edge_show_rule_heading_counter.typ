// Test: show rule that replaces heading with custom counter display
// typst#8228: function show rules remove HTML elements
// Pain point: converter sees the show-rule output (a block) instead of
// the original heading element — must recover heading semantics

#set heading(numbering: "1.")

// Custom heading show rule that adds decorative elements
#show heading.where(level: 1): it => {
  v(12pt)
  text(size: 16pt, weight: "bold")[
    #counter(heading).display("Chapter 1") #h(8pt) #it.body
  ]
  v(6pt)
  line(length: 100%, stroke: 0.5pt)
  v(6pt)
}

#show heading.where(level: 2): it => {
  text(size: 13pt, weight: "bold", fill: rgb("#333333"))[
    #counter(heading).display("1.1") #h(6pt) #it.body
  ]
  v(4pt)
}

= Introduction

Some introductory text.

== Background

Background details here.

== Motivation

Why this matters.

= Methods

Description of methods.

== Data Collection

How data was gathered.

= Results

The findings.
