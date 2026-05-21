// Test: footnote separator spacing
// Pandoc #7087: footnote separator overwrite causes extra spacing
// Pain point: separator paragraph needs w:spacing w:after="0"

#set page(height: 300pt, margin: (bottom: 60pt))
#set text(size: 10.5pt)

正文内容。#footnote[这是脚注1。]

更多正文内容。#footnote[这是脚注2。]

再一些正文。#footnote[包含*粗体*的脚注。]
