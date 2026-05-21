// Test: metadata fields with case variations
// Pandoc #5413: duplicate property names differing in case corrupt docx
// Pain point: docProps/custom.xml property names must be unique

#set document(
  title: "Case Test Document",
  author: "Test Author",
  keywords: ("keyword", "Keyword", "KEYWORD"),
)

This is a test document with potentially conflicting metadata.
