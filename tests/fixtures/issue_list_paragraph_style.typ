// Issue: pandoc#10974 — List items use "Normal" style instead of "List Paragraph"
// Word expects list items to use the "List Paragraph" style for proper spacing.
// typort should emit w:pStyle val="ListParagraph" for list items.

= Lists with Proper Styling

Paragraph before the list.

- First item in an unordered list
- Second item with some longer text that might wrap to the next line in narrow pages
- Third item

Paragraph between lists.

+ First ordered item
+ Second ordered item
+ Third ordered item

Paragraph after the lists. The vertical spacing between these paragraphs and the list items should match Word conventions.
