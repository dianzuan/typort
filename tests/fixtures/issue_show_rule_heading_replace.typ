// Test: function show rules on headings that replace semantic structure
// typst#8228: show rules remove HTML elements, <h1> becomes <div>
// Our PagedDocument recovery should still detect heading styles

#show heading: it => {
  text(size: 20pt, weight: "bold", it.body)
}

= First Heading

Body text after first heading.

== Second Heading

More body text.
