// Two forced line breaks in a row must survive as two <w:br/>s (a blank
// line), not be coalesced into one. Both the function and the markup form.
#set text(font: "Libertinus Serif")

first#linebreak()#linebreak()second

alpha \ \ beta
