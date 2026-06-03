// Regression fixture for recovery's "already present" detection (recovery.rs).
// A small page forces several pages of body. The align() title block is
// legitimately recovered from geometry (absent from HTML), which anchors the
// missing-content insertion at body index 0. Body lines carry inline math and
// superscript citations, so their paged text does not byte-match the doc and the
// pre-fix recovery misjudged them as "missing", injecting citation-number strings
// and duplicated body sentences as orphan paragraphs above the abstract.
#set page(width: 9cm, height: 5cm)
#set text(lang: "zh")

#align(center)[#text(size: 14pt)[标题块来自居中对齐]]

#par(first-line-indent: 0em)[摘要：本文把标量 $M$ 推广为矢量 $(M, v_i)$，其中 $v_i$ 表示方向偏好的内在属性。]

= 正文一

正文第一段，包含行内公式 $F(v)$ 与上标引用@smith2020，文字需要足够长以触发分页与逐行几何提取@knuth1997，从而让恢复器有机会把这些夹带公式与引文标记的行误判为缺失而重复注入@wang2023，继续填充文字使段落跨越多页并产生换行片段。

= 正文二

正文第二段同样含行内公式 $G(M)$ 与方向 $v_i$ 的论述，并再次引用@smith2020 以制造上标引文行，文字继续延展以确保版面跨页换行从而暴露恢复器的行匹配缺陷。

#bibliography("bibliography_basic.bib")
