// Test: CJK lang tag must set w:rFonts eastAsia attribute
// Pandoc #8451: CJK lang locks fonts to Word defaults without eastAsia
// Pain point: w:rFonts must set w:eastAsia alongside w:ascii/w:hAnsi

#set text(lang: "ja")

日本語のテスト文章です。English text mixed in.

#set text(lang: "zh")

中文测试段落。Also with English.
