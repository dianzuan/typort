// A superscript numeric citation style (here "nature", like the GB/T 7714
// numeric style) must render in-text citations as raised numbers — not as REF
// field codes pointing at bibliography keys (which have no Word bookmark and so
// render as "Error! Reference source not found").
As shown by prior work @smith2020 and a textbook @knuth1997, and also @wang2023.

#bibliography("bibliography_basic.bib", style: "nature")
