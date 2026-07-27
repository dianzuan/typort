//! Explicit `#pagebreak()` / `#colbreak()` recovery from the source AST.
//!
//! Both elements are consumed during compilation — in typst 0.15 they carry a
//! plain `#[elem]` (no introspection marker), so they are queryable in neither
//! the `HtmlDocument` nor the `PagedDocument`. Explicit breaks are therefore
//! recovered from the source, and positioned by **byte spans**, not by text:
//! every model run carries the `Span` of the source text that produced it, so
//! a break call at byte offset *B* belongs after the last element whose spans
//! end at or before *B*. Text matching (the previous approach) misplaced a
//! break whenever the preceding paragraph's text recurred earlier in the
//! document, and dropped it entirely when the break followed a heading, table
//! or figure.
//!
//! `#include`d files are followed per occurrence: content from an included file
//! sorts at each include site, so a break's document position is the *path* of
//! offsets from the main file down the include chain (lexicographically compared).
//! Local source functions are expanded at their call sites. Files reachable
//! only through `#import` get no include-chain position, however: content they
//! produce is function-mediated across file boundaries, which this static scan
//! cannot place reliably, so breaks inside them are skipped rather than guessed.
//!
//! Automatic page-flow boundaries (content that merely spilled onto the next
//! page) are deliberately NOT turned into hard breaks — they must reflow in
//! Word.

use std::collections::{HashMap, VecDeque};

use typort_ooxml::document::{BlockElement, Document, Paragraph, for_each_paragraph_in_block};
use typst::World;
use typst_library::foundations::PathOrStr;
use typst_syntax::ast::AstNode;
use typst_syntax::{FileId, LinkedNode, SyntaxKind, ast};

use crate::world::TyportWorld;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BreakKind {
    Page,
    Column,
}

/// A document-order position: byte offsets from the main file down the
/// `#include` chain. Lexicographic order == document order.
type PosKey = Vec<usize>;

/// Recover every explicit `#pagebreak()`/`#colbreak()` reachable from the main
/// source and insert the corresponding hard break into the document model.
pub(super) fn apply_breaks_from_source(world: &TyportWorld, doc: &mut Document) {
    // 1. Discover files breadth-first through string-literal `#include`s,
    //    assigning each include occurrence its position prefix, and collect
    //    every break call with its full position key.
    let main_id = world.main_source().id();
    let mut prefixes: HashMap<FileId, Vec<PosKey>> = HashMap::new();
    prefixes.insert(main_id, vec![Vec::new()]);
    let mut queue: VecDeque<(FileId, PosKey, Vec<FileId>)> =
        VecDeque::from([(main_id, Vec::new(), vec![main_id])]);
    let mut breaks: Vec<(PosKey, BreakKind)> = Vec::new();

    while let Some((fid, prefix, ancestors)) = queue.pop_front() {
        let Ok(source) = World::source(world, fid) else {
            continue;
        };
        let root = LinkedNode::new(source.root());
        let local_functions = collect_local_functions(&root);
        let mut state = ScanState {
            local_functions: &local_functions,
            bool_bindings: HashMap::new(),
            call_stack: Vec::new(),
            breaks: &mut breaks,
            on_include: &mut |path, offset| {
                // Resolve the include target exactly the way Typst does (relative
                // to the including file; `/`-prefixed relative to the root).
                let Ok(rooted) = PathOrStr::Str(path.into()).resolve(fid) else {
                    return;
                };
                let target = rooted.intern();
                if ancestors.contains(&target) {
                    return;
                }
                let mut key = prefix.clone();
                key.push(offset);
                prefixes.entry(target).or_default().push(key.clone());
                let mut child_ancestors = ancestors.clone();
                child_ancestors.push(target);
                queue.push_back((target, key, child_ancestors));
            },
        };
        scan_file(&root, &prefix, &mut state);
    }
    if breaks.is_empty() {
        return;
    }

    // 2. Key each body element by the latest position its runs came from.
    let mut occurrence_cursors = HashMap::new();
    let element_keys: Vec<Option<PosKey>> = doc
        .body
        .elements
        .iter()
        .map(|el| element_end_key(el, world, &prefixes, &mut occurrence_cursors))
        .collect();

    // 3. Each break inserts before the first keyed element positioned after
    //    it — i.e. after everything at or before the break. Elements without
    //    a key (recovery-inserted paragraphs, images) never anchor a break;
    //    they ride with their keyed neighbours.
    let mut inserts: Vec<(usize, BreakKind)> = breaks
        .into_iter()
        .map(|(bkey, kind)| {
            let idx = element_keys
                .iter()
                .position(|k| k.as_ref().is_some_and(|k| *k > bkey))
                .unwrap_or(element_keys.len());
            (idx, kind)
        })
        .collect();
    // Back-to-front so earlier insertion indices stay valid.
    inserts.sort_by_key(|(idx, _)| std::cmp::Reverse(*idx));
    for (idx, kind) in inserts {
        let mut br = Paragraph::new();
        match kind {
            BreakKind::Page => br.add_page_break(),
            BreakKind::Column => br.add_column_break(),
        }
        doc.body.elements.insert(idx, BlockElement::Paragraph(br));
    }
}

struct ScanState<'a, 'node> {
    local_functions: &'a HashMap<String, LinkedNode<'node>>,
    bool_bindings: HashMap<String, bool>,
    call_stack: Vec<String>,
    breaks: &'a mut Vec<(PosKey, BreakKind)>,
    on_include: &'a mut dyn FnMut(&str, usize),
}

/// Depth-first scan of one file: record break calls (offset appended to
/// `prefix`) and report every string-literal `#include` to `on_include`.
fn scan_file(node: &LinkedNode, prefix: &[usize], state: &mut ScanState<'_, '_>) {
    // A closure body is a definition, not executed document flow. Static source
    // scanning cannot know its runtime call count or position, so never invent a
    // hard break at the definition site.
    if node.kind() == SyntaxKind::Closure {
        return;
    }

    if matches!(
        node.kind(),
        SyntaxKind::ContentBlock | SyntaxKind::CodeBlock
    ) {
        let outer_bindings = state.bool_bindings.clone();
        for child in node.children() {
            scan_file(&child, prefix, state);
        }
        state.bool_bindings = outer_bindings;
        return;
    }

    if let Some(binding) = node.cast::<ast::LetBinding<'_>>()
        && let [name] = binding.kind().bindings().as_slice()
        && let Some(init) = binding.init()
        && let Some(value) = resolve_source_bool(init, &state.bool_bindings)
    {
        state.bool_bindings.insert(name.as_str().to_string(), value);
    }

    match selected_conditional_child(node, &state.bool_bindings) {
        ConditionalSelection::Child(child) => {
            scan_file(&child, prefix, state);
            return;
        }
        ConditionalSelection::Empty => return,
        ConditionalSelection::NotStatic => {}
    }

    match node.kind() {
        SyntaxKind::FuncCall => {
            if let Some(fc) = node.cast::<ast::FuncCall<'_>>()
                && let ast::Expr::Ident(ident) = fc.callee()
            {
                let kind = match ident.as_str() {
                    "pagebreak" => Some(BreakKind::Page),
                    "colbreak" => Some(BreakKind::Column),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let mut key = prefix.to_vec();
                    key.push(node.offset());
                    state.breaks.push((key, kind));
                } else if let Some(closure) = state.local_functions.get(ident.as_str()).cloned()
                    && !state.call_stack.iter().any(|name| name == ident.as_str())
                {
                    state.call_stack.push(ident.as_str().to_string());
                    let mut call_prefix = prefix.to_vec();
                    call_prefix.push(node.offset());
                    let outer_bindings = state.bool_bindings.clone();
                    for child in closure.children() {
                        scan_file(&child, &call_prefix, state);
                    }
                    state.bool_bindings = outer_bindings;
                    state.call_stack.pop();
                    return;
                }
            }
        }
        SyntaxKind::ModuleInclude => {
            if let Some(include) = node.cast::<ast::ModuleInclude<'_>>()
                && let ast::Expr::Str(s) = include.source()
            {
                (state.on_include)(&s.get(), node.offset());
            }
        }
        _ => {}
    }
    for child in node.children() {
        scan_file(&child, prefix, state);
    }
}

enum ConditionalSelection<'a> {
    NotStatic,
    Empty,
    Child(LinkedNode<'a>),
}

fn selected_conditional_child<'a>(
    node: &LinkedNode<'a>,
    bool_bindings: &HashMap<String, bool>,
) -> ConditionalSelection<'a> {
    let Some(conditional) = node.cast::<ast::Conditional<'_>>() else {
        return ConditionalSelection::NotStatic;
    };
    let Some(condition) = resolve_source_bool(conditional.condition(), bool_bindings) else {
        return ConditionalSelection::NotStatic;
    };
    let selected = if condition {
        Some(conditional.if_body())
    } else {
        conditional.else_body()
    };
    selected
        .and_then(|selected| {
            node.children()
                .find(|child| child.get() == selected.to_untyped())
        })
        .map_or(ConditionalSelection::Empty, ConditionalSelection::Child)
}

fn collect_local_functions<'a>(root: &LinkedNode<'a>) -> HashMap<String, LinkedNode<'a>> {
    fn visit<'a>(
        root: &LinkedNode<'a>,
        node: &LinkedNode<'a>,
        functions: &mut HashMap<String, LinkedNode<'a>>,
    ) {
        if let Some(binding) = node.cast::<ast::LetBinding<'_>>()
            && let [name] = binding.kind().bindings().as_slice()
            && let Some(ast::Expr::Closure(closure)) = binding.init()
            && let Some(linked) = root.find(closure.to_untyped().span())
        {
            functions.insert(name.as_str().to_string(), linked);
        }
        for child in node.children() {
            visit(root, &child, functions);
        }
    }

    let mut functions = HashMap::new();
    visit(root, root, &mut functions);
    functions
}

fn resolve_source_bool(expr: ast::Expr<'_>, bool_bindings: &HashMap<String, bool>) -> Option<bool> {
    match expr {
        ast::Expr::Bool(value) => Some(value.get()),
        ast::Expr::Ident(ident) => bool_bindings.get(ident.as_str()).copied(),
        _ => None,
    }
}

/// The latest source position among an element's runs, as a `PosKey` — the
/// element "ends" there in document order. `None` when no run resolves into a
/// file on the include chain (e.g. recovery-inserted content, or content
/// produced by an imported template function).
fn element_end_key(
    element: &BlockElement,
    world: &TyportWorld,
    prefixes: &HashMap<FileId, Vec<PosKey>>,
    occurrence_cursors: &mut HashMap<FileId, (usize, usize)>,
) -> Option<PosKey> {
    let mut best: Option<PosKey> = None;
    for_each_paragraph_in_block(element, &mut |para| {
        para.for_each_run(&mut |run| {
            let Some(span) = run.span else { return };
            let Some(fid) = span.id() else { return };
            let Some(file_prefixes) = prefixes.get(&fid) else {
                return;
            };
            let Some(range) = typst_library::WorldExt::range(world, span) else {
                return;
            };
            let cursor = occurrence_cursors.entry(fid).or_insert((0, 0));
            if range.start < cursor.1 && cursor.0 + 1 < file_prefixes.len() {
                cursor.0 += 1;
            }
            cursor.1 = range.end;
            let prefix = &file_prefixes[cursor.0];
            let mut key = prefix.clone();
            key.push(range.end);
            if best.as_ref().is_none_or(|b| key > *b) {
                best = Some(key);
            }
        });
    });
    best
}
