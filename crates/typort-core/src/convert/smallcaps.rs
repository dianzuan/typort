use super::{BlockElement, Document, HashSet, InlineElement, TyportWorld};

pub(super) fn apply_smallcaps_from_source(world: &TyportWorld, doc: &mut Document) {
    // SmallcapsElem is consumed during Typst realization — it doesn't survive
    // in the compiled Content AST. Detect it by walking the source AST for
    // function calls to `smallcaps` (or aliases defined via `#let sc = smallcaps`).
    let source_text = world.main_source().text();
    let sc_texts = extract_smallcaps_texts_from_ast(source_text);

    if sc_texts.is_empty() {
        return;
    }

    for element in &mut doc.body.elements {
        if let BlockElement::Paragraph(p) = element {
            for inline in &mut p.inlines {
                if let InlineElement::Text(run) = inline {
                    let trimmed = run.text.trim();
                    if sc_texts
                        .iter()
                        .any(|t| trimmed == *t || t.contains(trimmed))
                    {
                        run.smallcaps = true;
                    }
                }
            }
        }
    }
}

/// Extract text content from all `smallcaps` function calls in the source AST.
///
/// Uses `typst_syntax::parse` to walk the AST, which correctly handles:
/// - Direct calls: `#smallcaps[Hello]`
/// - Aliases: `#let sc = smallcaps; #sc[Hello]`
/// - Nested content: `#smallcaps[*bold* and _italic_]`
pub(super) fn extract_smallcaps_texts_from_ast(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);

    // First pass: find aliases for `smallcaps` (e.g., `#let sc = smallcaps`).
    let mut aliases: HashSet<String> = HashSet::new();
    aliases.insert("smallcaps".to_string());
    collect_smallcaps_aliases(&root, &mut aliases);

    // Second pass: find all function calls to smallcaps or its aliases,
    // and extract the text content from their content block arguments.
    let mut sc_texts = Vec::new();
    collect_smallcaps_call_texts(&root, &aliases, &mut sc_texts);
    sc_texts
}

/// Recursively find `#let X = smallcaps` bindings and add X to the alias set.
pub(super) fn collect_smallcaps_aliases(
    node: &typst_syntax::SyntaxNode,
    aliases: &mut HashSet<String>,
) {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::LetBinding
        && let Some(binding) = node.cast::<typst_syntax::ast::LetBinding<'_>>()
    {
        // Check if the init expression is an identifier that is `smallcaps` or an alias
        if let Some(typst_syntax::ast::Expr::Ident(init_ident)) = binding.init()
            && aliases.contains(init_ident.as_str())
        {
            // The binding names are the new aliases
            for ident in binding.kind().bindings() {
                aliases.insert(ident.as_str().to_string());
            }
        }
    }
    for child in node.children() {
        collect_smallcaps_aliases(child, aliases);
    }
}

/// Recursively find function calls to smallcaps (or aliases) and collect their text content.
pub(super) fn collect_smallcaps_call_texts(
    node: &typst_syntax::SyntaxNode,
    aliases: &HashSet<String>,
    texts: &mut Vec<String>,
) {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::FuncCall
        && let Some(call) = node.cast::<typst_syntax::ast::FuncCall<'_>>()
    {
        // Check if the callee is a smallcaps function or alias
        let is_smallcaps = match call.callee() {
            typst_syntax::ast::Expr::Ident(ident) => aliases.contains(ident.as_str()),
            _ => false,
        };
        if is_smallcaps {
            // Extract text from the content block argument
            for arg in call.args().items() {
                if let typst_syntax::ast::Arg::Pos(typst_syntax::ast::Expr::ContentBlock(block)) =
                    arg
                {
                    let text = collect_markup_text(block.body());
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        texts.push(trimmed);
                    }
                }
            }
        }
    }
    for child in node.children() {
        collect_smallcaps_call_texts(child, aliases, texts);
    }
}

/// Extract plain text content from a Markup AST node, stripping formatting.
pub(super) fn collect_markup_text(markup: typst_syntax::ast::Markup<'_>) -> String {
    use typst_syntax::ast::{AstNode, Expr};

    let mut result = String::new();
    for expr in markup.exprs() {
        match expr {
            Expr::Text(t) => result.push_str(t.get().as_str()),
            Expr::Space(_) => result.push(' '),
            Expr::Strong(s) => {
                let inner = collect_markup_text(s.body());
                result.push_str(&inner);
            }
            Expr::Emph(e) => {
                let inner = collect_markup_text(e.body());
                result.push_str(&inner);
            }
            _ => {
                // For other expression types, try to extract text from children
                let node = expr.to_untyped();
                result.push_str(&collect_text_from_syntax_node(node));
            }
        }
    }
    result
}

/// Recursively extract all text leaf content from a syntax node.
pub(super) fn collect_text_from_syntax_node(node: &typst_syntax::SyntaxNode) -> String {
    use typst_syntax::SyntaxKind;

    if node.kind() == SyntaxKind::Text || node.kind() == SyntaxKind::Space {
        return node.leaf_text().to_string();
    }
    let mut result = String::new();
    for child in node.children() {
        result.push_str(&collect_text_from_syntax_node(child));
    }
    result
}
