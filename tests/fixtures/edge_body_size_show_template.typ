#let helper() = {
  set text(size: 9pt)
  [helper text]
}
#let tmpl(body) = {
  set text(size: 12pt)
  body
}
#show: tmpl
= Heading
Body paragraph in twelve point, repeated so it dominates rendered size counts.
Body paragraph in twelve point, repeated so it dominates rendered size counts.
#block[
  #set text(size: 9pt)
  A small block at nine point.
]
