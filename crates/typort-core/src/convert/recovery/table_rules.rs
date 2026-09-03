//! Per-table border-rule recovery from paged frame evidence.

use std::collections::HashMap;

use typort_ooxml::document::{BlockElement, Document, TableBorders};
use typst::foundations::NativeElement;
use typst::introspection::Location;
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

/// Minimum Word border thickness in eighth-points.
const MIN_TABLE_RULE_EIGHTH_PT: u32 = 2;
/// Maximum minor-axis extent in points for a table rule.
const TABLE_RULE_AXIS_TOLERANCE_PT: f64 = 0.5;
/// Minimum horizontal table-rule length in points.
const MIN_HORIZONTAL_TABLE_RULE_LENGTH_PT: f64 = 40.0;
/// Minimum vertical table-rule length in points.
const MIN_VERTICAL_TABLE_RULE_LENGTH_PT: f64 = 8.0;

/// Per-table rule evidence harvested from the paged frames: horizontal rule
/// thicknesses (eighths of a point) and whether any cell-height vertical line
/// is drawn inside the table.
#[derive(Default)]
struct TableRules {
    sizes: Vec<u32>,
    has_vertical: bool,
}

/// Style each top-level table from the rules Typst actually drew FOR THAT
/// TABLE. The paged frames carry the introspection `Tag::Start`/`Tag::End`
/// brackets of every `TableElem`, so rule shapes are attributed to the table
/// whose bracket is open where they are painted — a footnote separator or an
/// author `#line()` (outside any bracket) can no longer restyle tables, and a
/// boxed table elsewhere no longer disables a genuine three-line table.
///
/// Per table: vertical lines → boxed (leave `borders` unset; the writer draws
/// a uniform grid), horizontal rules only → three-line (thick top/bottom,
/// thin header separator), no rules at all → the author drew the table
/// borderless (`stroke: none`); emit explicit nil borders so the writer's
/// uniform-grid fallback doesn't invent a box.
pub(in crate::convert) fn detect_three_line_tables(paged: &PagedDocument, doc: &mut Document) {
    let body_table_count = doc
        .body
        .elements
        .iter()
        .filter(|e| matches!(e, BlockElement::Table(_)))
        .count();
    if body_table_count == 0 {
        return;
    }

    let mut stack: Vec<Location> = Vec::new();
    let mut order: Vec<Location> = Vec::new();
    let mut per_table: HashMap<Location, TableRules> = HashMap::new();
    for page in paged.pages() {
        collect_table_rules(&page.frame, &mut stack, &mut order, &mut per_table);
    }

    // Document order of top-level paged tables must line up with the body's
    // table order; when it doesn't (a table the HTML walk dropped, or vice
    // versa), attribution would be misaligned — degrade to the writer's
    // uniform fallback rather than stamp the wrong table.
    if order.len() != body_table_count {
        return;
    }

    let mut locs = order.iter();
    for el in &mut doc.body.elements {
        let BlockElement::Table(t) = el else { continue };
        let Some(rules) = locs.next().and_then(|loc| per_table.get(loc)) else {
            continue;
        };
        if rules.has_vertical {
            // Boxed grid: keep `borders` unset — the writer's uniform fallback.
            continue;
        }
        if rules.sizes.is_empty() {
            // No rules drawn: `stroke: none`. Explicit nil borders on every side.
            t.borders = Some(TableBorders {
                top: None,
                bottom: None,
                left: None,
                right: None,
                inside_h: None,
                inside_v: None,
                header_sep: None,
                header_rows: 0,
            });
            continue;
        }
        let thin = *rules.sizes.iter().min().expect("non-empty");
        let thick = *rules.sizes.iter().max().expect("non-empty");
        t.borders = Some(TableBorders {
            top: Some(thick),
            bottom: Some(thick),
            left: None,
            right: None,
            inside_h: None,
            inside_v: None,
            header_sep: Some(thin),
            header_rows: 1,
        });
    }
}

/// Depth-first, in-paint-order walk attributing rule shapes to the innermost
/// open `TableElem` tag bracket. Only top-level brackets are recorded in
/// `order`/`per_table`; rules inside nested tables still count toward the
/// outer table's evidence (they ARE lines drawn within its region), and rules
/// outside every bracket are ignored entirely.
///
/// NB (evaluated 2026-07-12, typst 0.15): `Selector::within` cannot replace this
/// stack — it scopes introspector queries over *locatable elements*, while the
/// rule strokes attributed here are plain `FrameItem::Shape`s in the paged
/// frames, invisible to the introspector. Frame-order bracket matching is the
/// only source that pairs a drawn line with its table.
fn collect_table_rules(
    frame: &Frame,
    stack: &mut Vec<Location>,
    order: &mut Vec<Location>,
    per_table: &mut HashMap<Location, TableRules>,
) {
    use typst::introspection::Tag;
    super::super::frames::visit_frame_items(frame, false, &mut |_, item| {
        match item {
            FrameItem::Tag(Tag::Start(content, _)) => {
                if content.elem() == typst_library::model::TableElem::ELEM
                    && let Some(loc) = content.location()
                {
                    if stack.is_empty() {
                        order.push(loc);
                        per_table.entry(loc).or_default();
                    }
                    stack.push(loc);
                }
            }
            FrameItem::Tag(Tag::End(loc, ..)) => {
                if stack.last() == Some(loc) {
                    stack.pop();
                }
            }
            FrameItem::Shape(shape, _) => {
                let Some(&owner) = stack.first() else {
                    return; // not inside any table — footnote separator, #line(), …
                };
                if let typst::visualize::Geometry::Line(end) = &shape.geometry {
                    let dx = end.x.to_pt().abs();
                    let dy = end.y.to_pt().abs();
                    let thickness_pt = shape.stroke.as_ref().map_or(0.0, |s| s.thickness.to_pt());
                    if thickness_pt <= 0.0 {
                        return;
                    }
                    let sz = super::super::page::pt_to_eighth_pt(thickness_pt)
                        .max(MIN_TABLE_RULE_EIGHTH_PT);
                    let rules = per_table.entry(owner).or_default();
                    if dy < TABLE_RULE_AXIS_TOLERANCE_PT
                        && dx >= MIN_HORIZONTAL_TABLE_RULE_LENGTH_PT
                    {
                        // A wide, flat rule — a horizontal table line.
                        rules.sizes.push(sz);
                    } else if dx < TABLE_RULE_AXIS_TOLERANCE_PT
                        && dy >= MIN_VERTICAL_TABLE_RULE_LENGTH_PT
                    {
                        // A vertical line tall enough to be a cell border → boxed grid.
                        rules.has_vertical = true;
                    }
                }
            }
            _ => {}
        }
    });
}
