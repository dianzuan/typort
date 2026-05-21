// Test: layout elements (box, stack, h) dropped in HTML export
// typst#5966: box/stack/h silently dropped in HtmlDocument
// Pain point: content inside layout combinators missing from HTML,
// must be recovered from PagedDocument

Some text before layout.

Horizontal spacing: A B C

Some text after layout.
