// Regression: a document with NO explicit #pagebreak() must not get hard page
// breaks just because content flows onto a second page. A tall display equation
// that does not fit at the bottom of a page is pushed to the next page by Typst,
// leaving the previous page partly empty — which the old "page <95% full =>
// forced break" heuristic misread as a forced break and froze into a <w:br>.
// Automatic pagination must reflow in Word, so only explicit breaks count.
#set page(width: 8cm, height: 6cm)

这是第一段正文，填充足够多的文字让页面内容接近底部，目的是把后面那个较高的行间公式挤到放不下的位置，从而触发原来的误判逻辑。继续补充文字以占据版面空间，确保第一页被填到大半。

$ S = sum_(i=1)^n a_i dot.c b_i + integral_0^1 f(x) dif x + frac(partial W, partial E partial I) $

公式之后还有第二段正文，用于验证公式前后不应被硬分页符切开，而应在 Word 中自然回流。
