// Test: headings deeper than level 5
// typst#8121: level 6+ headings use <div role="heading"> instead of <hN>
// Pain point: HTML walker must handle both <hN> tags and div headings

= Level 1

== Level 2

=== Level 3

==== Level 4

===== Level 5

Content under level 5 heading.
