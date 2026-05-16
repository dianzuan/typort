#set text(size: 11pt)

= General Elements Test

== Code

Inline code: `hello_world()` and `x + y`.

Block code:

```rust
fn main() {
    println!("Hello, World!");
}
```

== Quotes

#quote(block: true)[
  This is a block quote. It should be indented and visually distinct from normal text.
]

== Term Lists

/ Rust: A systems programming language
/ Typst: A modern typesetting system
/ Word: A word processor by Microsoft

== Horizontal Rule

Some text above the rule.

#line(length: 100%)

Some text below the rule.

== Mixed Formatting

This paragraph has *bold*, _italic_, *_bold italic_*, and `inline code` together.
