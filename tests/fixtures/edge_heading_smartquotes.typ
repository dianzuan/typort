// Regression: smart quotes inside a heading were dropped. Heading text comes from
// the AST (extract_runs), where quotes are unresolved SmartQuoteElem nodes that
// walk_content silently skipped. They must resolve to curly quotes (open/close by
// context), like the body text already does.
#set text(lang: "zh")
#set heading(numbering: "一、")

= 关于"投资于人"的政策

正文段落，用于占据空间。

== 英文 "quoted" 子标题
