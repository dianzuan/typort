// Two separate ordered lists must each restart numbering at 1 (the second must
// not continue 4,5,6 from the first). They share one abstract numbering format,
// so every <w:num> instance needs a level-0 startOverride. ASCII-only / no font
// dependency so it is CI-safe.
First list:

+ alpha
+ beta
+ gamma

Some intervening paragraph separates the two lists.

Second list:

+ one
+ two
