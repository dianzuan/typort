// Test: paragraph split around block equations and tight lists
// typst#3206: paragraph cannot contain tight lists and block equations
// Pain point: single logical paragraph gets split into multiple <p> elements

The following items are important:
- Item one
- Item two

Consider the equation
$ x^2 + y^2 = r^2 $

A normal paragraph for comparison.
