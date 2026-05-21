// Test: bordered/framed content blocks
// Common in academic papers: callout boxes, algorithm frames, warnings
// Pain point: rect/block with stroke has no direct Word equivalent —
// must map to w:pBdr or w:tbl with borders

// Simple bordered block
#block(stroke: 1pt + black, inset: 10pt, width: 100%)[
  This content has a full border around it.
]

// Block with only left border (common for admonitions)
#block(stroke: (left: 3pt + blue), inset: (left: 12pt, y: 6pt), width: 100%)[
  *Note:* This is an important remark with a left accent border.
]

// Filled block with rounded corners
#block(fill: luma(240), inset: 10pt, radius: 4pt, width: 100%)[
  This is a gray background callout box often used for remarks or examples.
]

// Rect (explicit rectangle)
#rect(stroke: 2pt + red, inset: 8pt, width: 100%)[
  *Warning:* Handle with care. This text is inside a red-bordered rectangle.
]

// Nested bordered blocks
#block(stroke: 1pt, inset: 8pt, width: 100%)[
  Outer block content.
  #block(fill: luma(230), inset: 6pt, width: 100%)[
    Inner nested block with background.
  ]
  More outer content.
]
