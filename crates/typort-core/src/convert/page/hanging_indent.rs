use std::collections::HashMap;

use typst::World;
use typst_library::foundations::PathOrStr;

use crate::world::TyportWorld;

use super::source_ast::extract_show_template_names_from_source;
use super::units::{numeric_value_to_pt, pt_to_twips};

/// A `#set par(hanging-indent: …)` rule recovered from the source, including its
/// resolved literal/constant length and lexical scope.
pub struct ParHangingRule {
    pub offset: usize,
    pub nonzero: bool,
    pub em: Option<f64>,
    pub twips: Option<u32>,
    pub scope_end: Option<usize>,
}

#[derive(Debug, Copy, Clone)]
struct SourceLength {
    em: f64,
    pt: f64,
}

/// Collect every `#set par(hanging-indent: …)` rule from the main source, plus
/// imported length constants and document-template rules, in document order.
///
/// Unlike [`extract_source_style_overrides`] (which collapses to a single
/// document-wide value), a hanging-indent set-rule applies from its position
/// onward, so each occurrence *and* its byte position matter — a rule before a
/// hand-written reference list governs only the paragraphs that follow it. This
/// honors a value the author literally declared; it is not genre/keyword
/// matching. Main-source spans are read off the real `Source` tree so byte
/// offsets line up with the run spans used to locate each paragraph. An
/// imported document template contributes a global rule at offset zero.
#[must_use]
pub fn collect_par_hanging_indent_rules(world: &TyportWorld) -> Vec<ParHangingRule> {
    let source = world.main_source();
    let template_names = extract_show_template_names_from_source(source.text());
    let mut rules = collect_imported_template_hanging_length(world, source, &template_names)
        .map(|length| vec![par_hanging_rule(0, length, None)])
        .unwrap_or_default();
    let mut bindings = collect_imported_length_bindings(world, source);
    collect_hanging_rules(source, source.root(), None, &mut bindings, &mut rules);
    rules.sort_by_key(|r| r.offset);
    rules
}

fn collect_imported_template_hanging_length(
    world: &TyportWorld,
    source: &typst_syntax::Source,
    template_names: &[String],
) -> Option<SourceLength> {
    fn visit(
        world: &TyportWorld,
        source_id: typst_syntax::FileId,
        node: &typst_syntax::SyntaxNode,
        template_names: &[String],
        found: &mut Option<SourceLength>,
    ) {
        if let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
            && let typst_syntax::ast::Expr::Str(path) = import.source()
            && let Ok(rooted) = PathOrStr::Str(path.get().into()).resolve(source_id)
            && let Ok(imported) = World::source(world, rooted.intern())
        {
            let imported_names: Vec<String> = match import.imports() {
                Some(typst_syntax::ast::Imports::Wildcard) => template_names.to_vec(),
                Some(typst_syntax::ast::Imports::Items(items)) => items
                    .iter()
                    .filter(|item| {
                        template_names
                            .iter()
                            .any(|name| name == item.bound_name().as_str())
                    })
                    .map(|item| item.original_name().as_str().to_string())
                    .collect(),
                None => Vec::new(),
            };
            if !imported_names.is_empty()
                && let Some(length) = extract_template_hanging_length(&imported, &imported_names)
            {
                *found = Some(length);
            }
        }
        for child in node.children() {
            visit(world, source_id, child, template_names, found);
        }
    }

    let mut found = None;
    visit(
        world,
        source.id(),
        source.root(),
        template_names,
        &mut found,
    );
    found
}

fn extract_template_hanging_length(
    source: &typst_syntax::Source,
    template_names: &[String],
) -> Option<SourceLength> {
    fn visit(
        node: &typst_syntax::SyntaxNode,
        template_names: &[String],
        in_template_scope: bool,
        bindings: &mut HashMap<String, SourceLength>,
        found: &mut Option<SourceLength>,
    ) {
        use typst_syntax::SyntaxKind;
        use typst_syntax::ast;

        match node.kind() {
            SyntaxKind::ShowRule => return,
            SyntaxKind::ContentBlock => {
                for child in node.children() {
                    visit(child, template_names, false, bindings, found);
                }
                return;
            }
            SyntaxKind::LetBinding => {
                if let Some(binding) = node.cast::<ast::LetBinding<'_>>()
                    && let ast::LetBindingKind::Closure(name) = binding.kind()
                {
                    let child_scope = template_names.iter().any(|item| item == name.as_str());
                    let mut closure_bindings = bindings.clone();
                    for child in node.children() {
                        visit(
                            child,
                            template_names,
                            child_scope,
                            &mut closure_bindings,
                            found,
                        );
                    }
                    return;
                }
            }
            _ => {}
        }

        if in_template_scope
            && let Some(binding) = node.cast::<ast::LetBinding<'_>>()
            && let Some((name, length)) = resolve_length_binding(binding, bindings)
        {
            bindings.insert(name, length);
        }
        if in_template_scope
            && let Some(set) = node.cast::<ast::SetRule<'_>>()
            && matches!(set.target(), ast::Expr::Ident(ident) if ident.as_str() == "par")
        {
            for arg in set.args().items() {
                if let ast::Arg::Named(named) = arg
                    && named.name().as_str() == "hanging-indent"
                    && let Some(length) = resolve_source_length(named.expr(), bindings)
                {
                    *found = Some(length);
                }
            }
        }
        for child in node.children() {
            visit(child, template_names, in_template_scope, bindings, found);
        }
    }

    let mut bindings = collect_exported_length_bindings(source);
    let mut found = None;
    visit(
        source.root(),
        template_names,
        true,
        &mut bindings,
        &mut found,
    );
    found
}

fn collect_imported_length_bindings(
    world: &TyportWorld,
    source: &typst_syntax::Source,
) -> HashMap<String, SourceLength> {
    fn visit(
        world: &TyportWorld,
        source_id: typst_syntax::FileId,
        node: &typst_syntax::SyntaxNode,
        bindings: &mut HashMap<String, SourceLength>,
    ) {
        if let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
            && let typst_syntax::ast::Expr::Str(path) = import.source()
            && let Ok(rooted) = PathOrStr::Str(path.get().into()).resolve(source_id)
            && let Ok(imported) = World::source(world, rooted.intern())
        {
            let exported = collect_exported_length_bindings(&imported);
            match import.imports() {
                Some(typst_syntax::ast::Imports::Wildcard) => bindings.extend(exported),
                Some(typst_syntax::ast::Imports::Items(items)) => {
                    for item in items.iter() {
                        if let Some(length) = exported.get(item.original_name().as_str()) {
                            bindings.insert(item.bound_name().as_str().to_string(), *length);
                        }
                    }
                }
                None => {}
            }
        }
        for child in node.children() {
            visit(world, source_id, child, bindings);
        }
    }

    let mut bindings = HashMap::new();
    visit(world, source.id(), source.root(), &mut bindings);
    bindings
}

fn collect_exported_length_bindings(
    source: &typst_syntax::Source,
) -> HashMap<String, SourceLength> {
    fn visit(node: &typst_syntax::SyntaxNode, bindings: &mut HashMap<String, SourceLength>) {
        if matches!(
            node.kind(),
            typst_syntax::SyntaxKind::Closure
                | typst_syntax::SyntaxKind::ContentBlock
                | typst_syntax::SyntaxKind::CodeBlock
        ) {
            return;
        }
        if let Some(binding) = node.cast::<typst_syntax::ast::LetBinding<'_>>()
            && let Some((name, length)) = resolve_length_binding(binding, bindings)
        {
            bindings.insert(name, length);
        }
        for child in node.children() {
            visit(child, bindings);
        }
    }

    let mut bindings = HashMap::new();
    visit(source.root(), &mut bindings);
    bindings
}

/// Byte range of `span` within `source`.
///
/// typst 0.15 changed `Source::range` to take a decomposed
/// `(SpanNumber, Option<SubRange>)` instead of a `Span`; this performs that
/// decomposition (the same one `WorldExt::range` does) for a span that points
/// into `source`. Returns `None` for a detached span or one in another file.
fn span_range_in_source(
    source: &typst_syntax::Source,
    span: typst_syntax::Span,
) -> Option<std::ops::Range<usize>> {
    use typst_syntax::{DiagSpan, DiagSpanKind};
    match DiagSpan::from(span).get() {
        DiagSpanKind::Number { id, num, sub_range } if id == source.id() => {
            source.range(num, sub_range)
        }
        DiagSpanKind::Range { id, range } if id == source.id() => Some(range),
        _ => None,
    }
}

fn collect_hanging_rules(
    source: &typst_syntax::Source,
    node: &typst_syntax::SyntaxNode,
    inherited_scope_end: Option<usize>,
    bindings: &mut HashMap<String, SourceLength>,
    out: &mut Vec<ParHangingRule>,
) {
    use typst_syntax::SyntaxKind;

    let introduces_scope = matches!(
        node.kind(),
        SyntaxKind::ContentBlock | SyntaxKind::CodeBlock
    );
    let scope_end = if introduces_scope {
        span_range_in_source(source, node.span()).map(|range| range.end)
    } else {
        inherited_scope_end
    };

    // Set-rules nested inside a show-rule are element-scoped, not global.
    if node.kind() == SyntaxKind::ShowRule {
        return;
    }

    if node.kind() == SyntaxKind::LetBinding
        && let Some(binding) = node.cast::<typst_syntax::ast::LetBinding<'_>>()
        && let Some((name, length)) = resolve_length_binding(binding, bindings)
    {
        bindings.insert(name, length);
    }

    if node.kind() == SyntaxKind::SetRule
        && let Some(set) = node.cast::<typst_syntax::ast::SetRule<'_>>()
        && matches!(set.target(), typst_syntax::ast::Expr::Ident(i) if i.as_str() == "par")
    {
        for arg in set.args().items() {
            if let typst_syntax::ast::Arg::Named(named) = arg
                && named.name().as_str() == "hanging-indent"
                && let Some(length) = resolve_source_length(named.expr(), bindings)
                && let Some(range) = span_range_in_source(source, node.span())
            {
                out.push(par_hanging_rule(range.start, length, scope_end));
            }
        }
    }

    if introduces_scope {
        let mut scoped_bindings = bindings.clone();
        for child in node.children() {
            collect_hanging_rules(source, child, scope_end, &mut scoped_bindings, out);
        }
    } else {
        for child in node.children() {
            collect_hanging_rules(source, child, scope_end, bindings, out);
        }
    }
}

fn par_hanging_rule(
    offset: usize,
    length: SourceLength,
    scope_end: Option<usize>,
) -> ParHangingRule {
    ParHangingRule {
        offset,
        nonzero: length.em.abs() > f64::EPSILON || length.pt.abs() > f64::EPSILON,
        em: (length.em.abs() > f64::EPSILON).then_some(length.em),
        twips: (length.pt.abs() > f64::EPSILON).then(|| pt_to_twips(length.pt)),
        scope_end,
    }
}

fn resolve_length_binding(
    binding: typst_syntax::ast::LetBinding<'_>,
    bindings: &HashMap<String, SourceLength>,
) -> Option<(String, SourceLength)> {
    let introduced = binding.kind().bindings();
    let [name] = introduced.as_slice() else {
        return None;
    };
    let init = binding.init()?;
    let length = if let typst_syntax::ast::Expr::Closure(closure) = init {
        if closure.params().children().next().is_some() {
            return None;
        }
        resolve_source_length(closure.body(), bindings)?
    } else {
        resolve_source_length(init, bindings)?
    };
    Some((name.as_str().to_string(), length))
}

fn resolve_source_length(
    expr: typst_syntax::ast::Expr<'_>,
    bindings: &HashMap<String, SourceLength>,
) -> Option<SourceLength> {
    match expr {
        typst_syntax::ast::Expr::Numeric(numeric) => {
            let (value, unit) = numeric.get();
            match unit {
                typst_syntax::ast::Unit::Em => Some(SourceLength { em: value, pt: 0.0 }),
                typst_syntax::ast::Unit::Pt
                | typst_syntax::ast::Unit::Cm
                | typst_syntax::ast::Unit::Mm
                | typst_syntax::ast::Unit::In => Some(SourceLength {
                    em: 0.0,
                    pt: numeric_value_to_pt(value, unit),
                }),
                _ => None,
            }
        }
        typst_syntax::ast::Expr::Ident(ident) => bindings.get(ident.as_str()).copied(),
        typst_syntax::ast::Expr::Parenthesized(group) => {
            resolve_source_length(group.expr(), bindings)
        }
        typst_syntax::ast::Expr::FuncCall(call)
            if call.args().items().next().is_none()
                && matches!(call.callee(), typst_syntax::ast::Expr::Ident(_)) =>
        {
            let typst_syntax::ast::Expr::Ident(ident) = call.callee() else {
                return None;
            };
            bindings.get(ident.as_str()).copied()
        }
        typst_syntax::ast::Expr::Binary(binary) => match binary.op() {
            typst_syntax::ast::BinOp::Add | typst_syntax::ast::BinOp::Sub => {
                let lhs = resolve_source_length(binary.lhs(), bindings)?;
                let rhs = resolve_source_length(binary.rhs(), bindings)?;
                if binary.op() == typst_syntax::ast::BinOp::Add {
                    Some(SourceLength {
                        em: lhs.em + rhs.em,
                        pt: lhs.pt + rhs.pt,
                    })
                } else {
                    Some(SourceLength {
                        em: lhs.em - rhs.em,
                        pt: lhs.pt - rhs.pt,
                    })
                }
            }
            typst_syntax::ast::BinOp::Mul => {
                if let (Some(length), Some(scalar)) = (
                    resolve_source_length(binary.lhs(), bindings),
                    resolve_source_scalar(binary.rhs()),
                ) {
                    return Some(SourceLength {
                        em: length.em * scalar,
                        pt: length.pt * scalar,
                    });
                }
                let length = resolve_source_length(binary.rhs(), bindings)?;
                let scalar = resolve_source_scalar(binary.lhs())?;
                Some(SourceLength {
                    em: length.em * scalar,
                    pt: length.pt * scalar,
                })
            }
            typst_syntax::ast::BinOp::Div => {
                let length = resolve_source_length(binary.lhs(), bindings)?;
                let scalar = resolve_source_scalar(binary.rhs())?;
                (scalar.abs() > f64::EPSILON).then_some(SourceLength {
                    em: length.em / scalar,
                    pt: length.pt / scalar,
                })
            }
            _ => None,
        },
        _ => None,
    }
}

fn resolve_source_scalar(expr: typst_syntax::ast::Expr<'_>) -> Option<f64> {
    match expr {
        typst_syntax::ast::Expr::Int(value) => i32::try_from(value.get()).ok().map(f64::from),
        typst_syntax::ast::Expr::Float(value) => Some(value.get()),
        typst_syntax::ast::Expr::Parenthesized(group) => resolve_source_scalar(group.expr()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_hanging_indent_rules_records_set_and_reset() {
        // A non-zero rule then a reset (0em) rule are both recorded, in source
        // order, with the reset flagged `nonzero == false`.
        let src = typst_syntax::Source::detached(
            "Body.\n#set par(hanging-indent: 2em)\nRef one.\n\
             #set par(hanging-indent: 0em)\nBody again.\n",
        );
        let mut bindings = HashMap::new();
        let mut rules = Vec::new();
        collect_hanging_rules(&src, src.root(), None, &mut bindings, &mut rules);
        rules.sort_by_key(|rule| rule.offset);
        assert_eq!(rules.len(), 2, "two par(hanging-indent) rules expected");
        assert!(rules[0].nonzero, "first rule (2em) is a non-zero indent");
        assert!(!rules[1].nonzero, "second rule (0em) is a reset");
        assert!(
            rules[0].offset < rules[1].offset,
            "rules are ordered by source position"
        );
    }

    #[test]
    fn collect_hanging_indent_rules_ignores_unrelated_par_args() {
        // first-line-indent is not a hanging indent; no rule is recorded.
        let src = typst_syntax::Source::detached("#set par(first-line-indent: 2em)\nText.\n");
        let mut bindings = HashMap::new();
        let mut rules = Vec::new();
        collect_hanging_rules(&src, src.root(), None, &mut bindings, &mut rules);
        assert!(rules.is_empty());
    }
}
