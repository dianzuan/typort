// Test: document metadata beyond title/author
// Pandoc #3034: YAML metadata not mapped to docx custom properties
// Pain point: keywords and other metadata should be preserved

#set document(
  title: "Research Paper Title",
  author: ("Alice Smith", "Bob Jones"),
  date: datetime(year: 2024, month: 6, day: 15),
  keywords: ("machine learning", "optimization"),
)

= Abstract

This paper presents novel results.

= Introduction

Some introductory text.
