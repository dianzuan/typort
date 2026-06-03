// Regression fixture: an author-written par()[...] wrapper around inline math,
// the shape journal templates use for the abstract. Typst emits the nested par
// as a `Tag::Start("par")` whose prose lives in an inner Element<p>, interleaved
// with `equation` markers. handle_inline_tag had no "par" arm, so it skipped the
// whole range — dropping the prose and leaving only an orphan math paragraph.
// Kept on a small page so geometry recovery is not the variable under test.
#set page(width: 14cm, height: 10cm)

#par(first-line-indent: 0em)[
  #text[Lead: scalar $M$ becomes vector $(M, v_i)$, where $v_i$ matters here.]
]

= Body Heading

Flat body paragraph with inline math $A$ then more $B$ to lock the merge path.
