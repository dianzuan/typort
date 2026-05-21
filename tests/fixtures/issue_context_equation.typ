// Test: equations inside #context blocks are preserved
// typ2docx #52: equations captured in context are not extracted
// Pandoc/typst-hs #53: context keyword not supported

#context {
  let val = 42
  [The value is #val and the equation is $x^2 + y^2 = r^2$.]
}

Normal paragraph after context block.
