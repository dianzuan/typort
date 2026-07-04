// A #line() drawn by an imported template must still be recovered as a
// horizontal rule: the source gate scans all reachable files, not just main.
#import "imported_line_rule_tpl.typ": sep
#set text(font: "Libertinus Serif")

Text above the separator.

#sep()

Text below the separator.
