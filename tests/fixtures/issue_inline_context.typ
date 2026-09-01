// Regression: GitHub issue #4 (typort 0.1.1).
// A paragraph containing an *inline* `context` expression was emitted twice:
// once correct, once with the contextual content silently dropped.
// Block-level context was fine; only context mixed with sibling text broke.

X #context [ctx] tail.

#let q = counter("q")
#q.step()
Q #context q.display() first.

#q.step()
Q #context q.display() second.

#context [block-level ctx]
