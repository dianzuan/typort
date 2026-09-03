use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Extract file paths from `#import "path"` statements in Typst source.
///
/// Returns relative paths as written in the source (e.g. `"lib.typ"`).
#[must_use]
pub fn extract_import_paths(source: &str) -> Vec<String> {
    let root = typst_syntax::parse(source);
    let mut paths = Vec::new();
    collect_import_paths(&root, &mut paths);
    paths
}

fn collect_import_paths(node: &typst_syntax::SyntaxNode, paths: &mut Vec<String>) {
    use typst_syntax::SyntaxKind;
    if node.kind() == SyntaxKind::ModuleImport
        && let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
        && let typst_syntax::ast::Expr::Str(s) = import.source()
    {
        paths.push(s.get().to_string());
    }
    for child in node.children() {
        collect_import_paths(child, paths);
    }
}

/// String-literal `#import`/`#include` targets of a parsed source (one level).
fn collect_module_paths(node: &typst_syntax::SyntaxNode, paths: &mut Vec<String>) {
    use typst_syntax::SyntaxKind;
    match node.kind() {
        SyntaxKind::ModuleImport => {
            if let Some(import) = node.cast::<typst_syntax::ast::ModuleImport<'_>>()
                && let typst_syntax::ast::Expr::Str(s) = import.source()
            {
                paths.push(s.get().to_string());
            }
        }
        SyntaxKind::ModuleInclude => {
            if let Some(include) = node.cast::<typst_syntax::ast::ModuleInclude<'_>>()
                && let typst_syntax::ast::Expr::Str(s) = include.source()
            {
                paths.push(s.get().to_string());
            }
        }
        _ => {}
    }
    for child in node.children() {
        collect_module_paths(child, paths);
    }
}

/// The text of every local source file reachable from the main source through
/// string-literal `#import`/`#include` chains, the main text itself first.
/// Package imports (`@preview/...`) are skipped, cycles are visited once, and
/// paths resolve the way Typst resolves them: relative to the importing file's
/// directory, or to `root` when they start with `/`. Consumers that scan the
/// AST for a declaration (e.g. a template-drawn `#line()`) must scan ALL of
/// these — a declaration living in an imported template is just as
/// authoritative as one in the main file.
#[must_use]
pub fn collect_reachable_source_texts(
    root: &Path,
    main_dir: &Path,
    main_text: &str,
) -> Vec<String> {
    let mut texts = vec![main_text.to_string()];
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(main_dir.to_path_buf(), main_text.to_string())];

    while let Some((dir, text)) = stack.pop() {
        let mut paths = Vec::new();
        collect_module_paths(&typst_syntax::parse(&text), &mut paths);
        for path in paths {
            if path.starts_with('@') {
                continue; // package import — not a local file
            }
            let resolved = if let Some(rooted) = path.strip_prefix('/') {
                root.join(rooted)
            } else {
                dir.join(&path)
            };
            let canonical = resolved.canonicalize().unwrap_or(resolved);
            if !visited.insert(canonical.clone()) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&canonical) {
                let next_dir = canonical
                    .parent()
                    .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
                texts.push(content.clone());
                stack.push((next_dir, content));
            }
        }
    }
    texts
}
