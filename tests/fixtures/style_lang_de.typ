// Test: document language derived from #set text(lang:), not guessed from
// CJK presence. A German document must emit de-DE, not en-US/zh-CN.
#set text(lang: "de")

= Einleitung

Dies ist ein deutschsprachiges Dokument ohne ostasiatische Zeichen.
Die Sprache wird aus `#set text(lang: "de")` abgeleitet.
