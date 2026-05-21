// Test: figure placement (top, bottom, auto) and cross-references
// typst#8241: support fallback/priority-based figure placement
// typst#553: multi-column float elements
// Pain point: Typst place() vs Word anchoring model mismatch

#set heading(numbering: "1.")

= Introduction

Some introductory text before any figures appear. We reference
@fig:top and @fig:bottom below.

#figure(
  placement: top,
  table(columns: 3,
    [Method], [Precision], [Recall],
    [Ours], [0.95], [0.92],
    [Baseline], [0.88], [0.85],
  ),
  caption: [Performance comparison (placed at top).],
) <fig:top>

More body text between the two figures. This tests whether
the converter correctly handles figures that float away from
their source position.

#figure(
  placement: bottom,
  table(columns: 2,
    [Parameter], [Value],
    [$alpha$], [0.01],
    [$beta$], [0.95],
  ),
  caption: [Hyperparameters (placed at bottom).],
) <fig:bottom>

= Results

As shown in @fig:top, our method achieves higher precision.
The hyperparameters in @fig:bottom were tuned via grid search.
