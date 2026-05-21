// Issue: typst#6128 — URLs with CJK characters become broken in export
// When a link() contains CJK characters in the URL path, the encoding
// may be double-encoded, producing broken links in the output.

= Links with CJK Characters

Normal ASCII link: #link("https://example.com/page")[Example].

Link with CJK path: #link("https://example.com/文档/测试")[中文链接].

Link with mixed content: #link("https://example.com/docs/日本語")[Japanese docs].

Link with percent-encoded CJK: #link("https://example.com/%E6%96%87%E6%A1%A3")[Pre-encoded link].
