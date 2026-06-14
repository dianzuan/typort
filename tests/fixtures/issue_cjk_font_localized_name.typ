// Test: CJK fonts declared by their English family name — the only name Typst
// matches, since `info.family` is the English typographic name and the Chinese
// alias is not indexed — must reach Word as the LOCALIZED display name
// (SimSun → 宋体, SimHei → 黑体, KaiTi → 楷体), read from each font's own name
// table. Exercises page.rs `localize_cjk_fonts`. The body default localizes even
// though Typst may substitute a different face at render time (typst#6205).
#set text(font: ("Times New Roman", "SimSun"), lang: "zh", region: "cn")
#show heading: set text(font: ("Times New Roman", "SimHei"))

= 黑体标题内容

宋体正文内容这是一段测试段落文字 with Latin mixed in.

#text(font: ("Times New Roman", "KaiTi"), size: 14pt)[楷体大字内容内容]
