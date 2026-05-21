// Test: CJK text line breaks do not insert spurious spaces
// Pandoc #11241: line breaks turning into spaces is undesirable in CJK
// In CJK text, a newline should NOT become a space

#set text(lang: "zh")

这是一段中文文本，
用来测试换行是否会
插入不必要的空格。

正常的中文段落应该连续显示，
不应该在换行处出现空格。
