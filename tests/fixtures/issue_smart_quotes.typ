// Test: smart quotes and special punctuation preserved
// Pandoc #11463: curly quotes mangled in docx roundtrip
// Pandoc #11256: smart quotes do not honor lang specification

"Hello, world!" she said.

'Single quotes' are also important.

He said, "It's a 'nested' quote."

// Chinese quotation marks
#set text(lang: "zh")
「中文引号」和『书名号』测试。
