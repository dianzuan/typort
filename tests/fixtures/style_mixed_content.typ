// Test: mixed content — headings, lists, code, math, footnotes
// Expected: all style parameters correctly detected despite diverse content

#set text(size: 11pt)
#set par(justify: true)

= Introduction

This document tests mixed content types. The body text should be 11pt justified.

Here is a list:
- First item
- Second item with `inline code`
- Third item with math: $E = m c^2$

== Methods

We use the following equation#footnote[See reference for derivation.]:

$ integral_0^infinity f(x) dif x = 1 $

```python
def compute():
    return 42
```

=== Sub-section

Final paragraph with a #link("https://example.com")[link].
