// Test: spaces between adjacent styled runs must not be dropped
// Typst #5009: spaces at boundaries of bold/italic spans lost
// Pain point: w:t xml:space="preserve" must retain spaces between w:r elements

This has *bold* and _italic_ words with spaces.

Here is *bold* _italic_ adjacent.

A #text(fill: red)[red word] followed by normal.

Mix of *bold* and #underline[underlined] text.
