// Test: CJK and Latin font mixing in same paragraph
// Pandoc #7022/#9909: CJK font hints override Western fonts
// Pain point: Latin text in CJK document uses wrong font

#set text(lang: "zh")

中文正文中包含 English 单词和数字 2024。

引用"中文引号里有 English"测试。

公式 $E = m c^2$ 嵌入中文段落。
