// Test: function-generated content with headings and formatting
// typst-hs #81: early return from functions not working
// Pain point: heading returned from function must keep semantics

#let my-block() = {
  if true {
    return heading([Important Result], level: 2)
  }
  return strong([This should NOT appear])
}

#set heading(numbering: "1.")

= Introduction

Some text.

#my-block()

Body text after function-generated heading.
