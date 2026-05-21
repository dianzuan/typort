// Issue: typst#5778 — CJK heading numbering has too wide spacing
// When Chinese punctuation like 、is used in heading numbering,
// the 0.3em automatic space after numbering creates excessive gaps.
// typort should faithfully reproduce the spacing from the rendered result.

#set heading(numbering: "一、")
#set text(font: "Noto Serif CJK SC", lang: "zh")

= 引言

这是第一节的内容。

= 相关工作

这是第二节的内容。

#set heading(numbering: "1.")

= Section Three

This section uses Arabic numbering for comparison.

= Section Four

Another section with Arabic numbering.
