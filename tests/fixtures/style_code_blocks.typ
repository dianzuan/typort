// Test: code blocks and inline code
// Expected: monospace font detected from FontFlags::MONOSPACE

#set text(size: 11pt)

Here is some `inline code` in a paragraph.

```rust
fn main() {
    println!("Hello, world!");
    let x = 42;
}
```

And more text after the code block.

```python
def hello():
    print("Hello from Python")
    return True
```

Final paragraph.
