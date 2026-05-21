// Test: CJK heading numbering spacing
// typst#5778: too wide spacing between heading numbering and title in CJK
// Pain point: full-width punctuation + 0.3em gap may be too wide

#set text(lang: "zh")
#set heading(numbering: "一、")

= 第一章 绪论

正文内容。

== 研究背景

更多内容。
