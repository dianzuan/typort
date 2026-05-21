// Test: nested definition/term lists
// Pandoc #3379: nested definition lists lose indentation
// Pain point: each nesting level should have increased indent

/ Compiler: A program that translates source code.
  / Frontend: Lexing, parsing, semantic analysis.
    / Lexer: Breaks input into tokens.
    / Parser: Builds an AST from tokens.
  / Backend: Code generation and optimization.
/ Interpreter: Executes code directly without compilation.
