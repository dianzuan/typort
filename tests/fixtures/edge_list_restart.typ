// Test: separate numbered lists restart numbering
// Pandoc #8367: lists continue counting instead of restarting
// This is a critical real-world bug in many converters

+ First list item one
+ First list item two
+ First list item three

This paragraph separates two independent lists.

+ Second list item one
+ Second list item two

Another separator paragraph.

+ Third list item one
