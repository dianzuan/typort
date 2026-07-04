// A #pagebreak() inside an #include'd file must still be recovered: the
// source scan follows the include chain, and the break's document position
// is keyed through the include site.
#set text(font: "Libertinus Serif")

#include "pagebreak_include_ch1.typ"

Chapter two text.
