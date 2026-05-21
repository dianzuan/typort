// Test: heading with numbering via show rule
// Common academic pattern: numbered headings via show rule + counter
// Should preserve heading text even when show rule adds counter prefix

#set heading(numbering: "1.1")

= Introduction

Some text.

== Background

More text.

== Methods

Final text.
