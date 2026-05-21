// Test: cross-reference bookmark IDs longer than 40 chars
// Pandoc #5091: Word has 40-char limit on bookmark names
// Pain point: long labels break internal cross-references

#set heading(numbering: "1.")

= A very long heading title <very-long-heading-label-name-exceeds-forty>

Some text here.

= Short heading <short>

See @very-long-heading-label-name-exceeds-forty and @short.
