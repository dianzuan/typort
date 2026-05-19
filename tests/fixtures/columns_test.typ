// Test: Two-column layout.
#set page(columns: 2, height: 15cm)

= Two Column Test

This is a test of two-column layout. The text should flow into two columns on the page. We need enough content to demonstrate the column behavior and fill both columns.

Column layouts are commonly used in academic papers and newspapers. They allow more text to fit on a page while maintaining a readable line length. This is important for readability and professional appearance.

When converting to Word format, columns should be represented using the `w:cols` element in the section properties, with the number of columns and spacing specified. The conversion needs to detect the column layout from the rendered output.

This paragraph provides additional content to ensure both columns are populated with text for a proper visual test of the conversion. We need the text to wrap from the first column into the second column.

More text fills the second column area to verify proper column detection and conversion from the Typst layout to OOXML format. The column detection algorithm analyzes the x-positions of text items on the page.

Additional content is needed to ensure the text flows into the second column. Without enough content, the text might fit entirely in the first column, making column detection impossible.

The final paragraph provides more text to guarantee that both columns contain content. This is essential for the column detection algorithm to identify the two distinct text position clusters that indicate a multi-column layout.
