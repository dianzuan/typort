// Regression fixture for recover_missing_content (recovery.rs) "site 2" (the
// CJK-projection dedup). The old code stripped a hardcoded Chinese-numeral prefix
// off the paged line before projecting to CJK ideographs, which OVER-SUPPRESSED a
// legitimate layout-only (#align(center)) enumerated line whose title prose also
// appeared in body text: "三、甲乙丙，丁戊己" was deleted because its number-stripped
// projection "甲乙丙丁戊己" substring-matched the abstract's "甲乙丙丁戊己". Dropping the
// numeral strip (the fix) leaves the leading numeral in the projection ("三甲乙丙丁
// 戊己"), which no longer collides — so the distinct centered line is preserved
// (recovered), not silently deleted. The body run "甲乙丙丁戊己" has no comma; the
// centered line's "甲乙丙，丁戊己" does, so it is uniquely identifiable in the output.
#set page(width: 9cm, height: 6cm)
#set text(lang: "zh")

#par(first-line-indent: 0em)[摘要：本研究讨论甲乙丙丁戊己等六个核心要素，需要一定篇幅的正文以产生分页与换行片段，从而让恢复器扫描到后续的几何行并尝试去重匹配，继续填充文字使版面跨越多页。]

#align(center)[三、甲乙丙，丁戊己]

正文继续延展以触发分页与逐行几何提取，从而让恢复器有机会扫描行并尝试去重匹配，继续填充文字使段落跨越多页并产生换行片段，文字继续延展占满版面以复现缺陷情形。
