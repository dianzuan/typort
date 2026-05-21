// Issue: typst#8100 — HTML whitespace preservation in content sequences
// Trailing/leading spaces inside styled spans (highlight, underline, etc.)
// may be collapsed in HTML export, causing words to merge.
// typort relies on HtmlDocument, so this can cause run-together text.

= Whitespace in Styled Spans

#highlight[Hello ] World should have a space after "Hello".

#underline[Hello ] World should also preserve the trailing space.

#text(fill: red)[Red text ] followed by normal text.

#strong[Bold ] then #emph[italic ] then normal.

Mixed: #highlight[highlighted ]and#highlight[ highlighted] with spaces.
