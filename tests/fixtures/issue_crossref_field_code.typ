// Issue: pandoc#6078 — Cross-references should use Word REF field codes
// instead of plain hyperlinks, so Word can update numbering dynamically.
// typort already uses REF field codes for @label; this fixture tests
// that figure/table/equation references all produce proper field codes.

#set heading(numbering: "1.")
#set math.equation(numbering: "(1)")

= Introduction <intro>

See @fig-chart for the figure and @tbl-data for the table.
Also refer to @eq-main and @intro.

= Methods

#figure(
  rect(width: 80%, height: 40pt, fill: luma(230))[Chart placeholder],
  caption: [A sample chart],
) <fig-chart>

#figure(
  table(
    columns: 2,
    [*X*], [*Y*],
    [1], [2],
    [3], [4],
  ),
  caption: [Sample data table],
) <tbl-data>

$ f(x) = integral_0^x t^2 dif t $ <eq-main>

= Discussion

As shown in @fig-chart, the results from @tbl-data confirm @eq-main.
