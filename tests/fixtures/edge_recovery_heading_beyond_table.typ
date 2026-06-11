// Regression fixture for recover_missing_content (recovery.rs): a SHORT heading
// whose Typst-computed number falls OUTSIDE the old hardcoded Chinese-numeral
// table (一..十五). The 16th heading renders "十六、". Pre-fix, the short-line
// (<6 chars) gate skipped the whitespace-cancelled full-text dedup, and the
// CN_NUMS path capped at 十五, so "十六、讨论" matched no dedup path and was
// re-scraped from page geometry and injected as a DUPLICATE orphan paragraph.
// The fix dedups heading lines against the emitted heading text — which already
// carries the same Typst number — with whitespace cancelled, language-agnostically
// (no numeral table), so the orphan no longer appears.
#set page(width: 9cm, height: 5cm)
#set text(lang: "zh")
#set heading(numbering: "一、")

#align(center)[#text(size: 14pt)[标题块来自居中对齐]]

#par(first-line-indent: 0em)[摘要：本段用于触发逐行几何恢复，需要一定篇幅的正文以产生分页与换行片段，从而让恢复器扫描到后续的标题行并尝试匹配。]

#counter(heading).update(15)

= 讨论

正文第一段需要足够长以触发分页与逐行几何提取，从而让恢复器有机会扫描标题行并尝试去重匹配，继续填充文字使段落跨越多页并产生换行片段，以复现短标题被误判为缺失内容而重复注入的缺陷，文字继续延展占满版面。

= 综述

正文第二段继续延展以确保版面跨页换行，进一步暴露恢复器对编号超出旧硬编码数字表的短标题的处理缺陷，文字继续延展以制造多页布局并产生更多逐行几何样本供恢复器扫描。
