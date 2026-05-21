// Test: block elements inside table cells
// tinymist #1845 / typst #8308: block elements in table cells break structure
// Pain point: code blocks, lists, math in cells may be hoisted out

#table(
  columns: (1fr, 1fr),
  [Header 1], [Header 2],
  [
    ```python
    def hello():
        print("world")
    ```
  ],
  [Regular text content],
  [
    - Item one
    - Item two
    - Item three
  ],
  [
    $ sum_(i=1)^n x_i $
  ],
)
