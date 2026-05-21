// Test: text transformations — smallcaps, upper, lower, alternating styles
// typst#7009: fake small caps simulation
// typst#2465: setting text case
// Pain point: Word smallcaps is w:smallCaps, but Typst may synthesize it

// Small caps
This has #smallcaps[Small Caps Text] in the middle.

// All small caps
#smallcaps(all: true)[All Letters Are Small Caps Here]

// Upper case transformation
#upper[this text should be uppercase]

// Lower case transformation
#lower[THIS TEXT SHOULD BE LOWERCASE]

// Combining transformations with formatting
*#smallcaps[Bold Small Caps]*

_#upper[italic uppercase]_

// Small caps in a heading
= #smallcaps[Chapter Title in Small Caps]

This is body text after a small-caps heading.

// Mixed: CJK text with transformations
// (upper/lower only affect Latin characters)
#upper[混合 Mixed case 文本 text]
