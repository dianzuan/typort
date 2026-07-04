// With author margins smaller than Typst's default, layout-only content near
// the page edge must still be recovered: the body-zone boundary must use the
// document's actual margins, not the default (which would classify the top
// ~64pt of every page as "header" and silently drop the banner below).
#set page(margin: 1cm)
#set text(font: "Libertinus Serif")

First page body text.

#pagebreak()

#place(top + left)[PAGE-TWO-TOP-BANNER-UNIQUE]
#v(1cm)

Second page body text.
