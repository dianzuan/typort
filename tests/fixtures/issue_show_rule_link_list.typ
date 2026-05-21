// Test: show rule on links inside list items
// typst-hs #63: show rule applied twice to content in list
// Pain point: show-rule + hyperlink + list combination

#show link: underline

- #link("https://example.com")[Example] for testing
- Regular item without link
- Another #link("https://example.com")[link] here

Normal paragraph with #link("https://example.com")[link] outside list.
