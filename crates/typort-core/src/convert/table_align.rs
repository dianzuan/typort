//! Faithful table-cell alignment from the semantic `TableElem`.
//!
//! Typst's HTML export emits bare `<td>` with no alignment, so cell alignment is
//! lost on the HTML path. The alignment the author actually set lives in the
//! semantic `TableElem` (queried from the introspector), so we read it there and
//! apply it to the IR: HTML supplies table *structure*, the introspector supplies
//! the *styling* the HTML dropped — mirroring Pandoc's AST→writer split, adapted to
//! typort's three-source model.
//!
//! Nothing here is hardcoded: each cell gets exactly the alignment Typst resolves
//! for it (per-cell override, else the table-level value/array/closure), or nothing
//! when the author leaves it `auto`.

use typort_ooxml::document::{Alignment, CellContent, Table, TableCell, VMerge, VerticalAlign};
use typst::World;
use typst::comemo::Track;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Smart, StyleChain};
use typst::introspection::Introspector;
use typst_html::HtmlDocument;
use typst_library::layout::{Alignment as TstAlignment, Celled, HAlignment, VAlignment};
use typst_library::model::{TableCell as TstCell, TableChild, TableElem, TableItem};
use typst_utils::Protected;

use crate::world::TyportWorld;
/// Apply each cell's authored alignment (from the semantic `TableElem`) to the IR
/// table — vertical → `vertical_align` (`w:vAlign`), horizontal → the cell
/// paragraphs' alignment (`w:jc`). Cells the author left unaligned are untouched.
pub(super) fn apply_cell_alignment(
    table: &mut Table,
    table_elem: &TableElem,
    world: &TyportWorld,
    html_doc: &HtmlDocument,
) {
    let semantic_cells = collect_cells(table_elem);
    let table_align = table_elem.align.get_ref(StyleChain::default());
    let mut sink = Sink::new();
    let traced = Traced::default();
    let introspector: &dyn Introspector = &**html_doc.introspector();
    let mut engine = Engine {
        world: (world as &dyn World).track(),
        library: world.library(),
        introspector: Protected::new(introspector.track()),
        traced: traced.track(),
        sink: sink.track_mut(),
        route: Route::default(),
    };

    // Walk the IR's origin cells (skipping `vMerge` continuation placeholders, which
    // have no semantic counterpart) in document order, alongside the semantic cells.
    let mut sem = semantic_cells.iter();
    for (row_idx, row) in table.rows.iter_mut().enumerate() {
        let mut col = 0usize;
        for cell in &mut row.cells {
            if cell.vmerge == VMerge::Continue {
                col += cell.colspan as usize;
                continue;
            }
            let Some(sc) = sem.next() else { return };
            if let Some(align) = resolve_align(sc, table_align, col, row_idx, &mut engine) {
                apply_align(cell, align);
            }
            col += cell.colspan as usize;
        }
    }
}

/// Flatten the table's semantic cells in document order (headers/footers included,
/// `hline`/`vline` skipped), matching the HTML cell order the IR was built from.
fn collect_cells(table_elem: &TableElem) -> Vec<TstCell> {
    let mut out = Vec::new();
    for child in &table_elem.children {
        match child {
            TableChild::Item(item) => push_cell(item, &mut out),
            TableChild::Header(header) => {
                for item in &header.children {
                    push_cell(item, &mut out);
                }
            }
            TableChild::Footer(footer) => {
                for item in &footer.children {
                    push_cell(item, &mut out);
                }
            }
        }
    }
    out
}

fn push_cell(item: &TableItem, out: &mut Vec<TstCell>) {
    if let TableItem::Cell(cell) = item {
        out.push((**cell).clone());
    }
}

/// Resolve a cell's alignment: a per-cell override wins; otherwise the table-level
/// alignment resolved by Typst at the cell coordinate. Returns `None` for `auto`
/// or when evaluating the authored closure fails.
fn resolve_align(
    cell: &TstCell,
    table_align: &Celled<Smart<TstAlignment>>,
    col: usize,
    row: usize,
    engine: &mut Engine,
) -> Option<TstAlignment> {
    if let Smart::Custom(a) = cell.align.get_ref(StyleChain::default()) {
        return Some(*a);
    }
    match table_align
        .resolve(engine, StyleChain::default(), col, row)
        .ok()?
    {
        Smart::Custom(align) => Some(align),
        Smart::Auto => None,
    }
}

/// Vertical → `vertical_align` (`w:vAlign`); horizontal → cell paragraphs' alignment
/// (`w:jc`), exactly as Pandoc does horizontal table alignment.
fn apply_align(cell: &mut TableCell, align: TstAlignment) {
    if let Some(v) = align.y() {
        cell.vertical_align = Some(match v {
            VAlignment::Top => VerticalAlign::Top,
            VAlignment::Horizon => VerticalAlign::Center,
            VAlignment::Bottom => VerticalAlign::Bottom,
        });
    }
    if let Some(h) = align.x() {
        let ha = match h {
            HAlignment::Left | HAlignment::Start => Alignment::Left,
            HAlignment::Center => Alignment::Center,
            HAlignment::Right | HAlignment::End => Alignment::Right,
        };
        for para in &mut cell.paragraphs {
            para.alignment = Some(ha);
        }
        for item in &mut cell.content {
            if let CellContent::Paragraph(para) = item {
                para.alignment = Some(ha);
            }
        }
    }
}
