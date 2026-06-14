// Test: composite symbol with subscript
// typst-hs #41: subscript on composite symbol fails
// Pain point: symbol variant via dot notation then subscript
//
// Note: `plus.circle` (⊕) was renamed to `plus.o` in typst 0.15's symbol table;
// the test's intent (subscript on a dot-notation composite symbol) is unchanged.

$ plus.o_2 $

$ arrow.r.double^n $

$ dots.c_k $
