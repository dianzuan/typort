// `first-line-indent: (amount: 2em, all: true)` indents EVERY paragraph,
// including the first one after a heading (the Chinese-typography convention).
// With the bare `first-line-indent: 2em` shorthand (all: false, the Typst
// default), Typst — and typort — suppress that first paragraph's indent.

#set par(first-line-indent: (amount: 2em, all: true))

= Heading One

First paragraph right after the heading must still be first-line indented here.

Second paragraph is indented too as usual in this document body for the test.
