#set heading(numbering: "1.")

= 第一章 引言 <intro>

这是正文段落，包含*加粗*和_斜体_。

== 背景 <background>

如@intro 所述，本文探讨数字经济。见@fig-demo。

#figure(
  image("spike_test.png", width: 50%),
  caption: [测试图片],
) <fig-demo>

#figure(
  table(
    columns: 3,
    [变量], [含义], [单位],
    [X], [GDP], [万亿],
    [Y], [人口], [亿人],
  ),
  caption: [变量说明表],
) <tbl-vars>

如@tbl-vars 所示，变量定义如下。

行内公式 $x^2 + y^2 = z^2$ 和块公式：

$ sum_(i=1)^n i = frac(n(n+1), 2) $

这里有一个脚注#footnote[这是脚注内容，用于测试。]。
