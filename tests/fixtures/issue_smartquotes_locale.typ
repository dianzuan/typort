// Test: smartquotes in non-English locales
// typst#6853: incorrect smartquotes after html.frame in non-English locales
// Pain point: quote direction may be corrupted in certain locales

#set text(lang: "fr")
"Citation en français"

#set text(lang: "de")
"Deutsches Zitat"

#set text(lang: "en")
"English quote"
