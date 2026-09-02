use std::io::{self, Write};

use quick_xml::Writer;

use super::WriteCtx;
use super::paragraph::write_paragraph;
use crate::document::{CellContent, Table, VMerge};

#[allow(clippy::too_many_lines, reason = "writes the schema-ordered table XML")]
pub(super) fn write_table<W: Write>(
    writer: &mut Writer<W>,
    table: &Table,
    ctx: &WriteCtx,
) -> io::Result<()> {
    // Determine number of columns from the first row for equal-width distribution
    let num_cols = table
        .rows
        .first()
        .map_or(1, |r| r.cells.iter().map(|c| c.colspan).sum::<u32>());

    writer.create_element("w:tbl").write_inner_content(|w| {
        // Table properties with borders
        let tbl_width = table.width_pct.unwrap_or(5000).to_string();
        w.create_element("w:tblPr").write_inner_content(|tpr| {
            tpr.create_element("w:tblW")
                .with_attribute(("w:w", tbl_width.as_str()))
                .with_attribute(("w:type", "pct"))
                .write_empty()?;
            tpr.create_element("w:tblBorders")
                .write_inner_content(|bdr| {
                    // Per-side thicknesses: from detected `borders` when present
                    // (so a three-line table stays three-line), else a uniform grid.
                    let uniform = Some(table.border_size.unwrap_or(4));
                    // CT_TblBorders is an xsd:sequence — child order is fixed:
                    // top, left, bottom, right, insideH, insideV.
                    let sides: [(&str, Option<u32>); 6] = match &table.borders {
                        Some(tb) => [
                            ("w:top", tb.top),
                            ("w:left", tb.left),
                            ("w:bottom", tb.bottom),
                            ("w:right", tb.right),
                            ("w:insideH", tb.inside_h),
                            ("w:insideV", tb.inside_v),
                        ],
                        None => [
                            ("w:top", uniform),
                            ("w:left", uniform),
                            ("w:bottom", uniform),
                            ("w:right", uniform),
                            ("w:insideH", uniform),
                            ("w:insideV", uniform),
                        ],
                    };
                    for (name, sz) in sides {
                        let el = bdr.create_element(name);
                        if let Some(sz) = sz {
                            let s = sz.to_string();
                            el.with_attribute(("w:val", "single"))
                                .with_attribute(("w:sz", s.as_str()))
                                .with_attribute(("w:space", "0"))
                                .write_empty()?;
                        } else {
                            el.with_attribute(("w:val", "nil")).write_empty()?;
                        }
                    }
                    Ok(())
                })?;
            Ok(())
        })?;
        // Table rows
        for (row_idx, row) in table.rows.iter().enumerate() {
            w.create_element("w:tr").write_inner_content(|tr_w| {
                for cell in &row.cells {
                    tr_w.create_element("w:tc").write_inner_content(|tc_w| {
                        // Emit cell properties (width, merges)
                        let has_colspan = cell.colspan > 1;
                        let has_vmerge = cell.vmerge != VMerge::None;
                        // Always emit tcPr to set cell width
                        tc_w.create_element("w:tcPr").write_inner_content(|tcpr| {
                            // Cell width: use explicit value or equal distribution
                            let cell_width = cell
                                .width_pct
                                .unwrap_or_else(|| (5000 / num_cols) * cell.colspan);
                            let width_str = cell_width.to_string();
                            tcpr.create_element("w:tcW")
                                .with_attribute(("w:w", width_str.as_str()))
                                .with_attribute(("w:type", "pct"))
                                .write_empty()?;
                            if has_colspan {
                                let span_str = cell.colspan.to_string();
                                tcpr.create_element("w:gridSpan")
                                    .with_attribute(("w:val", span_str.as_str()))
                                    .write_empty()?;
                            }
                            if has_vmerge {
                                match &cell.vmerge {
                                    VMerge::Restart => {
                                        tcpr.create_element("w:vMerge")
                                            .with_attribute(("w:val", "restart"))
                                            .write_empty()?;
                                    }
                                    VMerge::Continue => {
                                        tcpr.create_element("w:vMerge").write_empty()?;
                                    }
                                    VMerge::None => {}
                                }
                            }
                            // Three-line header separator: a bottom rule on the
                            // cells of the last header row (insideH is off, so the
                            // line appears under the header only).
                            if let Some(tb) = &table.borders
                                && let Some(sep) = tb.header_sep
                                && tb.header_rows > 0
                                && row_idx + 1 == tb.header_rows
                            {
                                let s = sep.to_string();
                                tcpr.create_element("w:tcBorders")
                                    .write_inner_content(|tbw| {
                                        tbw.create_element("w:bottom")
                                            .with_attribute(("w:val", "single"))
                                            .with_attribute(("w:sz", s.as_str()))
                                            .with_attribute(("w:space", "0"))
                                            .write_empty()?;
                                        Ok(())
                                    })?;
                            }
                            // Vertical cell alignment (w:vAlign comes after
                            // tcBorders in the CT_TcPr schema order).
                            if let Some(va) = cell.vertical_align {
                                tcpr.create_element("w:vAlign")
                                    .with_attribute(("w:val", va.ooxml_value()))
                                    .write_empty()?;
                            }
                            Ok(())
                        })?;
                        if !cell.content.is_empty() {
                            // Cell has structured content (paragraphs + nested tables)
                            let mut has_trailing_para = false;
                            for item in &cell.content {
                                match item {
                                    CellContent::Paragraph(para) => {
                                        write_paragraph(tc_w, para, ctx)?;
                                        has_trailing_para = true;
                                    }
                                    CellContent::Table(nested_tbl) => {
                                        write_table(tc_w, nested_tbl, ctx)?;
                                        has_trailing_para = false;
                                    }
                                }
                            }
                            // OOXML requires a trailing w:p after a nested w:tbl in a cell
                            if !has_trailing_para {
                                tc_w.create_element("w:p").write_empty()?;
                            }
                        } else if cell.paragraphs.is_empty() {
                            // OOXML requires at least one paragraph per cell
                            tc_w.create_element("w:p").write_empty()?;
                        } else {
                            for para in &cell.paragraphs {
                                write_paragraph(tc_w, para, ctx)?;
                            }
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(())
}
