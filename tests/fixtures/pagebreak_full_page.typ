// Test: pagebreak after content that fills most of the page.
// The old heuristic (85% threshold) would miss this break.
#set page(height: 400pt, margin: 30pt)

// With 400pt page and 30pt margins, content area is 340pt.
// Use vertical space to fill ~90% of the page, then text.
#v(300pt)

First page content.

#pagebreak()

This is on page two.
