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
//! `#include`d files are followed: content from an included file sorts at the
//! include site, so a break's document position is the *path* of offsets from
//! the main file down the include chain (lexicographically compared). Files
//! reachable only through `#import` get no such position — content they
//! produce is function-mediated (one source call site, arbitrarily many
//! runtime instances), which no static source scan can place; breaks inside
//! them are skipped rather than guessed.
//!
//! Automatic page-flow boundaries (content that merely spilled onto the next
//! page) are deliberately NOT turned into hard breaks — they must reflow in
//! Word.

use std::collections::{HashMap, VecDeque};

use typort_ooxml::document::{BlockElement, Document, Paragraph, for_each_paragraph_in_block};
use typst::World;
use typst_library::foundations::PathOrStr;
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
    //    assigning each file its include-site position prefix, and collect
    //    every break call with its full position key.
    let main_id = world.main_source().id();
    let mut prefixes: HashMap<FileId, PosKey> = HashMap::new();
    prefixes.insert(main_id, Vec::new());
    let mut queue: VecDeque<FileId> = VecDeque::from([main_id]);
    let mut breaks: Vec<(PosKey, BreakKind)> = Vec::new();

    while let Some(fid) = queue.pop_front() {
        let Ok(source) = World::source(world, fid) else {
            continue;
        };
        let prefix = prefixes[&fid].clone();
        let root = LinkedNode::new(source.root());
        scan_file(&root, &prefix, &mut breaks, &mut |path, offset| {
            // Resolve the include target exactly the way Typst does (relative
            // to the including file; `/`-prefixed relative to the root).
            let Ok(rooted) = PathOrStr::Str(path.into()).resolve(fid) else {
                return;
            };
            let target = rooted.intern();
            if let std::collections::hash_map::Entry::Vacant(entry) = prefixes.entry(target) {
                let mut key = prefix.clone();
                key.push(offset);
                entry.insert(key);
                queue.push_back(target);
            }
        });
    }
    if breaks.is_empty() {
        return;
    }

    // 2. Key each body element by the latest position its runs came from.
    let element_keys: Vec<Option<PosKey>> = doc
        .body
        .elements
        .iter()
        .map(|el| element_end_key(el, world, &prefixes))
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

/// Depth-first scan of one file: record break calls (offset appended to
/// `prefix`) and report every string-literal `#include` to `on_include`.
fn scan_file(
    node: &LinkedNode,
    prefix: &[usize],
    breaks: &mut Vec<(PosKey, BreakKind)>,
    on_include: &mut dyn FnMut(&str, usize),
) {
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
                    breaks.push((key, kind));
                }
            }
        }
        SyntaxKind::ModuleInclude => {
            if let Some(include) = node.cast::<ast::ModuleInclude<'_>>()
                && let ast::Expr::Str(s) = include.source()
            {
                on_include(&s.get(), node.offset());
            }
        }
        _ => {}
    }
    for child in node.children() {
        scan_file(&child, prefix, breaks, on_include);
    }
}

/// The latest source position among an element's runs, as a `PosKey` — the
/// element "ends" there in document order. `None` when no run resolves into a
/// file on the include chain (e.g. recovery-inserted content, or content
/// produced by an imported template function).
fn element_end_key(
    element: &BlockElement,
    world: &TyportWorld,
    prefixes: &HashMap<FileId, PosKey>,
) -> Option<PosKey> {
    let mut best: Option<PosKey> = None;
    for_each_paragraph_in_block(element, &mut |para| {
        para.for_each_run(&mut |run| {
            let Some(span) = run.span else { return };
            let Some(fid) = span.id() else { return };
            let Some(prefix) = prefixes.get(&fid) else {
                return;
            };
            let Some(range) = typst_library::WorldExt::range(world, span) else {
                return;
            };
            let mut key = prefix.clone();
            key.push(range.end);
            if best.as_ref().is_none_or(|b| key > *b) {
                best = Some(key);
            }
        });
    });
    best
}
