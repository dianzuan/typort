use super::{
    CellContent, Document, HtmlDocument, HtmlElement, HtmlNode, InlineFmt, InlineOptions, Location,
    Paragraph, StyleChain, Table, TableCell, TableElem, TableRow, Tag, TyportWorld, VMerge,
    WalkCtx, collect_inlines, content_at_location, element_at_location, find_first_element,
    find_tag_end, get_attr_value, image, is_inline_equation_at, subtree_has_element, table_align,
    table_width, tag_name, walk_tags,
};

/// Rowspan metadata for a single cell: `(html_cell_index, rowspan, colspan)`.
pub(super) type CellSpanInfo = (usize, u32, u32);

/// A parsed table row paired with its rowspan metadata.
pub(super) type RawTableRow = (TableRow, Vec<CellSpanInfo>);
pub(super) fn handle_table_tag(
    children: &[HtmlNode],
    i: usize,
    location: Location,
    ctx: &mut WalkCtx,
) -> usize {
    let end = find_tag_end(children, i, location);
    handle_table(&children[i..=end], Some(location), ctx);
    end
}

/// Handle a `table` Tag: find the HTML `<table>` element in the inner children and parse it.
pub(super) fn handle_table(slice: &[HtmlNode], table_loc: Option<Location>, ctx: &mut WalkCtx) {
    if let Some(table) = find_first_element(slice, &|element| tag_name(element) == "table") {
        convert_html_table(table, table_loc, ctx.doc, ctx.html_doc, ctx.world);
        return;
    }
    // Fallback: walk inner children normally
    let inner = &slice[1..slice.len().saturating_sub(1)];
    walk_tags(inner, ctx);
}

/// Convert an HTML `<table>` element into the document model.
pub(super) fn convert_html_table(
    elem: &HtmlElement,
    table_loc: Option<Location>,
    doc: &mut Document,
    html_doc: &HtmlDocument,
    world: &TyportWorld,
) {
    let Some(mut table) = convert_html_table_to_model(elem, html_doc) else {
        return;
    };

    // Semantic column widths: read the declared track sizes off the TableElem
    // and turn them into per-cell percentages. Degrades to equal distribution
    // (cells left at width_pct = None) when the spec is all-`Auto`/`columns: N`,
    // or when the element is not queryable (e.g. nested tables with no location).
    if let Some(loc) = table_loc
        && let Some(table_elem) = element_at_location::<TableElem>(html_doc, loc)
    {
        let tracks = table_elem.columns.get_ref(StyleChain::default());
        let content_pt = f64::from(
            doc.page_settings
                .width_twips
                .saturating_sub(doc.page_settings.margin_left)
                .saturating_sub(doc.page_settings.margin_right),
        ) / 20.0;
        let wctx = table_width::TableWidthCtx {
            content_pt,
            body_font_pt: f64::from(doc.style.body_size_half_pt) / 2.0,
        };
        if let Some(col_pct) = table_width::track_widths_pct(&tracks.0, wctx) {
            table_width::assign_cell_widths(&mut table, &col_pct);
        }
        // Semantic cell alignment (the HTML `<td>`s carry none): horizontal → cell
        // paragraph `w:jc`, vertical → `w:vAlign`, read from the same TableElem.
        table_align::apply_cell_alignment(&mut table, &table_elem, world, html_doc);
    }

    doc.add_table(table);
}

/// Post-process table rows to insert `VMerge::Continue` cells for rowspans.
///
/// In HTML, when a cell has `rowspan=N`, the subsequent N-1 rows omit the cell at that
/// column position. In OOXML, every row must have the same number of logical columns,
/// and continuation cells must have `<w:vMerge/>` (no val = continue).
pub(super) fn postprocess_rowspans(raw_rows: Vec<RawTableRow>) -> Table {
    // Track active rowspans: (logical_col_index, rows_remaining, colspan)
    // `rows_remaining` counts how many MORE rows need a continuation cell.
    let mut active_spans: Vec<(usize, u32, u32)> = Vec::new();
    let mut final_rows = Vec::new();

    for (row, span_info) in raw_rows {
        // Sort active spans by column index
        active_spans.sort_by_key(|(col, _, _)| *col);

        // Build the new row by interleaving continuation cells with source cells
        let mut new_cells = Vec::new();
        let mut logical_col: usize = 0;
        let mut src_idx: usize = 0;
        let src_cells = row.cells;

        loop {
            // Check if this logical column needs a continuation cell
            if let Some(&(_, _, colspan)) = active_spans.iter().find(|(c, _, _)| *c == logical_col)
            {
                new_cells.push(TableCell {
                    paragraphs: vec![Paragraph::new()],
                    content: Vec::new(),
                    colspan,
                    vmerge: VMerge::Continue,
                    width_pct: None,
                    vertical_align: None,
                });
                logical_col += colspan as usize;
            } else if src_idx < src_cells.len() {
                logical_col += src_cells[src_idx].colspan as usize;
                new_cells.push(src_cells[src_idx].clone());
                src_idx += 1;
            } else {
                break;
            }
        }

        // Decrement active spans and remove expired ones (AFTER using them for this row)
        active_spans.retain_mut(|(_, remaining, _)| {
            *remaining -= 1;
            *remaining > 0
        });

        // Register new rowspans from this row's span_info.
        // span_info indices are relative to the HTML source cells; remap to logical positions.
        for (html_col_idx, rowspan, colspan) in &span_info {
            // Find the logical column for this HTML cell index by walking the final cells,
            // skipping continuation cells (which don't correspond to HTML source cells).
            let mut logical = 0_usize;
            let mut html_idx = 0_usize;
            for cell in &new_cells {
                if cell.vmerge == VMerge::Continue {
                    logical += cell.colspan as usize;
                    continue;
                }
                if html_idx == *html_col_idx {
                    break;
                }
                logical += cell.colspan as usize;
                html_idx += 1;
            }
            if *rowspan > 1 {
                active_spans.push((logical, *rowspan - 1, *colspan));
            }
        }

        final_rows.push(TableRow { cells: new_cells });
    }

    Table {
        rows: final_rows,
        width_pct: None,
        border_size: None,
        borders: None,
    }
}

/// Convert a `<tr>` element into a `TableRow` plus rowspan metadata.
///
/// Returns `(TableRow, Vec<(cell_index, rowspan, colspan)>)` where the second
/// element records which cells have `rowspan > 1` so the caller can insert
/// `VMerge::Continue` cells in subsequent rows.
pub(super) fn convert_table_row(tr: &HtmlElement, html_doc: &HtmlDocument) -> Option<RawTableRow> {
    let mut cells = Vec::new();
    let mut span_info = Vec::new();
    let mut cell_idx: usize = 0;

    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                let is_header = tag == "th";
                // Check if <td> children include <p> elements for multi-paragraph cells
                let paragraphs = convert_cell_paragraphs(td, is_header, html_doc);

                // Parse colspan and rowspan attributes
                let colspan = get_attr_value(td, "colspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let rowspan = get_attr_value(td, "rowspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);

                let vmerge = if rowspan > 1 {
                    VMerge::Restart
                } else {
                    VMerge::None
                };

                if rowspan > 1 {
                    span_info.push((cell_idx, rowspan, colspan));
                }

                // Check for nested tables within the cell
                let (final_paragraphs, cell_content) =
                    extract_cell_content_with_nested_tables(td, html_doc, paragraphs);

                cells.push(TableCell {
                    paragraphs: final_paragraphs,
                    content: cell_content,
                    colspan,
                    vmerge,
                    width_pct: None,
                    vertical_align: None,
                });
                cell_idx += 1;
            }
        }
    }
    if cells.is_empty() {
        None
    } else {
        Some((TableRow { cells }, span_info))
    }
}

/// Convert a `<td>` or `<th>` element's children into paragraphs.
///
/// If the cell contains `<p>` child elements, each `<p>` becomes a separate paragraph.
/// Otherwise, all inline content is collected into a single paragraph.
pub(super) fn convert_cell_paragraphs(
    td: &HtmlElement,
    is_header: bool,
    html_doc: &HtmlDocument,
) -> Vec<Paragraph> {
    // Every paragraph collected below shares the same formatting: bold iff this
    // is a header cell, nothing else set.
    let fmt = InlineFmt {
        bold: is_header,
        ..InlineFmt::default()
    };

    // Typst's HTML export drops every equation, leaving inline math as `equation`
    // Tag siblings between the cell's <p> text fragments. The per-<p> path below
    // would consume only the <p>s — dropping those equation siblings and stacking
    // a single math-bearing line into several paragraphs. When the cell carries
    // inline math, collect the whole cell as one paragraph instead, so the
    // equations are spliced back in document order. The shared inline collector
    // already turns an `equation` Tag into OMML and recurses through <p> wrappers
    // to pick up the surrounding text.
    let has_inline_equation =
        (0..td.children.len()).any(|i| is_inline_equation_at(&td.children, i, html_doc));
    if has_inline_equation {
        let mut para = Paragraph::new();
        collect_inlines(
            &td.children,
            &mut para,
            None,
            InlineOptions::generic(fmt, Some(html_doc)),
        );
        return vec![para];
    }

    // Check if any direct children are <p> elements
    let has_p_children = td.children.iter().any(|c| {
        if let HtmlNode::Element(el) = c {
            tag_name(el) == "p"
        } else {
            false
        }
    });

    if has_p_children {
        let mut paragraphs = Vec::new();
        for child in &td.children {
            if let HtmlNode::Element(el) = child
                && tag_name(el) == "p"
            {
                let mut para = Paragraph::new();
                collect_inlines(
                    &el.children,
                    &mut para,
                    None,
                    InlineOptions::generic(fmt, Some(html_doc)),
                );
                if !para.inlines.is_empty() {
                    paragraphs.push(para);
                }
            }
        }
        if paragraphs.is_empty() {
            // Fallback: collect all content as one paragraph
            let mut para = Paragraph::new();
            collect_inlines(
                &td.children,
                &mut para,
                None,
                InlineOptions::generic(fmt, Some(html_doc)),
            );
            vec![para]
        } else {
            paragraphs
        }
    } else {
        let mut para = Paragraph::new();
        collect_inlines(
            &td.children,
            &mut para,
            None,
            InlineOptions::generic(fmt, Some(html_doc)),
        );
        vec![para]
    }
}

/// Check if a `<td>`/`<th>` element contains nested `<table>` elements and,
/// if so, build a `Vec<CellContent>` that interleaves paragraphs and nested
/// tables in document order.
///
/// Returns `(paragraphs, content)` where:
/// - `paragraphs` is the original paragraph list (kept for backward compat)
/// - `content` is non-empty only when nested tables are present
pub(super) fn extract_cell_content_with_nested_tables(
    td: &HtmlElement,
    html_doc: &HtmlDocument,
    paragraphs: Vec<Paragraph>,
) -> (Vec<Paragraph>, Vec<CellContent>) {
    // Check if any child (direct or nested in a wrapper div/span) is a <table>
    // `subtree_has_element` also sees tables represented as flattened
    // `Tag::Start` markers, which an element-only walk missed.
    let has_nested_table = subtree_has_element(&td.children, "table");
    if !has_nested_table {
        return (paragraphs, Vec::new());
    }

    // Walk children in order, collecting paragraphs and nested tables
    let mut content: Vec<CellContent> = Vec::new();
    collect_cell_content_recursive(&td.children, html_doc, &mut content);

    // Also build the flat paragraphs list for backward compat
    let flat_paragraphs: Vec<Paragraph> = content
        .iter()
        .filter_map(|c| {
            if let CellContent::Paragraph(p) = c {
                Some(p.clone())
            } else {
                None
            }
        })
        .collect();

    let final_paragraphs = if flat_paragraphs.is_empty() {
        vec![Paragraph::new()]
    } else {
        flat_paragraphs
    };

    (final_paragraphs, content)
}

/// Recursively collect cell content (paragraphs and nested tables) from HTML
/// children, preserving document order.
pub(super) fn collect_cell_content_recursive(
    children: &[HtmlNode],
    html_doc: &HtmlDocument,
    content: &mut Vec<CellContent>,
) {
    for child in children {
        match child {
            HtmlNode::Element(el) => {
                let tag = tag_name(el);
                if tag == "table" {
                    // Convert this as a nested table
                    let table = convert_html_table_to_model(el, html_doc);
                    if let Some(t) = table {
                        content.push(CellContent::Table(t));
                    }
                } else if tag == "p" {
                    let mut para = Paragraph::new();
                    collect_inlines(
                        &el.children,
                        &mut para,
                        None,
                        InlineOptions::generic(InlineFmt::default(), Some(html_doc)),
                    );
                    if !para.inlines.is_empty() {
                        content.push(CellContent::Paragraph(para));
                    }
                } else if tag != "math" {
                    // Recurse into wrapper elements (div, span, etc.). A bare
                    // `<math>` (equation outside a `<p>`) is skipped: its OMML
                    // comes from the sibling equation Tag below — recursing
                    // would leak the MathML glyphs as literal cell text.
                    collect_cell_content_recursive(&el.children, html_doc, content);
                }
            }
            HtmlNode::Text(text, _) => {
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.add_run(trimmed);
                    content.push(CellContent::Paragraph(para));
                }
            }
            HtmlNode::Tag(tag) => {
                // A bare equation in the cell (outside any `<p>`): convert it
                // through the introspector like the inline collector does.
                if let Tag::Start(c, _) = tag
                    && c.elem().name() == "equation"
                    && let Some(eq) = content_at_location(html_doc, tag.location())
                {
                    let omml = typort_math::equation_to_omml(&eq);
                    let mut para = Paragraph::new();
                    para.add_math(omml);
                    content.push(CellContent::Paragraph(para));
                }
            }
            HtmlNode::Frame(frame) => {
                // Layouted-opaque content in a cell: rasterize in place.
                if let Some(img) = image::rasterize_html_frame(frame) {
                    let mut para = Paragraph::new();
                    para.add_image(img);
                    content.push(CellContent::Paragraph(para));
                }
            }
        }
    }
}

/// Convert an HTML `<table>` element into a `Table` model (without adding to doc).
/// Returns `None` if the table has no rows.
pub(super) fn convert_html_table_to_model(
    elem: &HtmlElement,
    html_doc: &HtmlDocument,
) -> Option<Table> {
    let raw_rows = collect_table_rows(elem, html_doc);
    (!raw_rows.is_empty()).then(|| postprocess_rowspans(raw_rows))
}

/// Collect direct and section-wrapped HTML table rows once for every table path.
pub(super) fn collect_table_rows(elem: &HtmlElement, html_doc: &HtmlDocument) -> Vec<RawTableRow> {
    let mut raw_rows = Vec::new();
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                if let Some(result) = convert_table_row(row_or_section, html_doc) {
                    raw_rows.push(result);
                }
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                        && let Some(result) = convert_table_row(tr, html_doc)
                    {
                        raw_rows.push(result);
                    }
                }
            }
        }
    }
    raw_rows
}
