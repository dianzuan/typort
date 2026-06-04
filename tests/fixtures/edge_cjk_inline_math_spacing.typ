// Regression: typort's inline-equation merge inserts a space run before/after each
// inline equation (Typst trims the source space in its HTML fragments). For Latin
// text that space is needed ("the value x is"); for CJK it is wrong — Typst renders
// 标量M推广 tight, and a literal space reads as an artificial artifact.
#set text(lang: "zh")

中文段落标量$M$推广为矢量$v$结束。

English paragraph: the value $x$ is here.
