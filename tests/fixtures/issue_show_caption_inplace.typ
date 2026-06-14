#set text(lang: "en")
#set page(width: 12cm, height: 16cm, margin: 1.5cm)
#set figure(numbering: "1")
#show figure.caption: it => [#it.supplement #context it.counter.display(it.numbering) -- #it.body]
= Introduction
Para before any figure. This body paragraph must remain first.
#figure(rect(width: 3cm, height: 2cm, fill: aqua), caption: [Caption alpha one]) <fig-a>
Para between the two figures, which must keep its slot.
#figure(rect(width: 3cm, height: 2cm, fill: lime), caption: [Caption beta two]) <fig-b>
Para after both figures, near the end of the document.
