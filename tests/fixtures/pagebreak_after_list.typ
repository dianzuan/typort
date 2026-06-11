// A #pagebreak() placed after a list (and before the next heading) must land
// AFTER the last list item, not before the list. List-item text is nested markup,
// so the source-AST anchor must descend into the item. ASCII-only / CI-safe.
= Conclusions

Body paragraph before the list.

+ First recommendation item.
+ Second recommendation item.
+ Last recommendation item.

#pagebreak()

= References
