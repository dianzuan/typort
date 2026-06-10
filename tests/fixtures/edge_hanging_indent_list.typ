// A list under a `#set par(hanging-indent: …)` rule must keep its own list
// indent (left 2em / hanging 1em), not be clobbered with the bibliography
// hanging indent (2em / 2em). Inline markup gives the runs source spans, which
// is what used to route the list items through the hanging-indent pass.

#set par(hanging-indent: 2em)

- A list item with *bold* text long enough to wrap onto a second line here now.
- Another list item with _emphasis_ that also wraps across the available width.
