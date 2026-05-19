// Test: Section breaks from page setting changes.
// The first section uses default A4 settings,
// then changes to US Letter with different margins.

= First Section

This is content in the first section with default A4 page settings.

Some body text to fill the page.

#page(width: 21.59cm, height: 27.94cm, margin: (x: 3cm, y: 3cm))[
  = Second Section

  This is content in the second section with US Letter page settings and different margins.

  Some body text in the second section.
]
