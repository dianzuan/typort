// Regression: a figure/table caption must appear exactly once. The recovery
// pass must not re-insert captions as duplicate paragraphs. This used to rely
// on hardcoded "图 "/"表 " keyword skipping; it must now hold via semantic
// text dedup, language-agnostically. CJK caption supplements ("图", "表")
// exercise the path the old keyword filter special-cased.
#set text(lang: "zh", region: "CN", font: ("Noto Serif CJK SC",))
#set page(width: 12cm, height: 16cm, margin: 1.5cm)

= 引言

正文段落,在图表之前。

#figure(
  rect(width: 3cm, height: 2cm, fill: aqua),
  caption: [一个矩形示意图的标题],
) <fig:rect>

#figure(
  table(columns: 2, [甲], [乙], [1], [2]),
  caption: [实验数据汇总表],
) <tab:data>

图表之后的正文。
