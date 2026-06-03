// Regression: a long left-aligned heading whose text happens to span most of the
// line has a text-center near the page center, so the geometry alignment detector
// (page.rs apply_paragraph_alignment) misclassified it as centered. A left-aligned
// line starts at the left margin (min_x ≈ left margin), which distinguishes it
// from a genuinely centered line.
= 这是一个相当长的左对齐二级标题用于触发文本中心接近页面中心的居中误判问题

正文段落内容，确保文档有正文，并且左对齐标题应当保持左对齐而不是被误判为居中显示。

= 另一个同样很长的左对齐标题继续验证多个标题都不会被错误地识别成居中对齐方式
