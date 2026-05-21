// Issue: typst#8228 — Function show rules remove HTML semantic elements
// When a show rule replaces a heading with custom block content,
// the heading semantics (h1/h2) are lost in HTML export.
// typort uses HtmlDocument, so this directly affects us:
// headings with show rules may appear as plain divs instead of headings.

#set heading(numbering: "1.")

#show heading: it => {
  set text(weight: "bold")
  block(
    below: 0.8em,
    counter(heading).display(it.numbering) + h(8pt) + it.body
  )
}

= First Heading

Some paragraph text here.

== Second Heading

More paragraph text under the subheading.

=== Third Heading

Final paragraph.
