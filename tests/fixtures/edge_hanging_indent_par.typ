// `#set par(hanging-indent: …)` is a declared value (source AST), not a genre
// heuristic: paragraphs after the rule get a hanging indent; the paragraph
// before it stays flush. Language-neutral.

First paragraph before the rule has no hanging indent at all in this document.

#set par(hanging-indent: 2em)

Second paragraph after the rule should carry a hanging indent for its lines.

Third paragraph after the rule should also carry a hanging indent for wrapping.
