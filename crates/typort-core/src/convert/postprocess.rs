use super::{BlockElement, CellContent, Document, HtmlDocument, ParagraphStyle, Table};

/// Post-processing: suppress first-line indent on the first paragraph after
/// each heading.
///
/// Bibliography hanging indent is applied only to *real* bibliographies — those
/// Typst emits with the `doc-bibliography` role from `#bibliography(...)` (see
/// the `"section"` arm of `handle_html_element`). Hand-written paragraphs that
/// merely look like a reference list are, to Typst, ordinary text, so typort
/// converts them as ordinary text rather than guessing from heading keywords
/// (which would assume the document's language — see CLAUDE.md language-neutrality rules).
pub(super) fn apply_paragraph_formatting(doc: &mut Document) {
    let mut after_heading = false;
    let mut is_first_element = true;
    // When the source declared `first-line-indent: (.., all: true)`, EVERY
    // paragraph is indented — including the first after a heading — so we must
    // not suppress it (the Typst default `all: false` does suppress it).
    let indent_all = doc.style.first_line_indent_all;

    for element in &mut doc.body.elements {
        if let BlockElement::Paragraph(p) = element {
            if matches!(p.style, Some(ParagraphStyle::Heading(_))) {
                // Suppress above-spacing on the first heading (Typst collapses
                // block(above) with page margin at page start).
                if is_first_element {
                    p.spacing_before = Some(0);
                }
                after_heading = true;
            } else {
                // Normal paragraph
                if after_heading {
                    p.suppress_indent = !indent_all;
                    after_heading = false;
                }
            }
            is_first_element = false;
        } else if let BlockElement::Table(t) = element {
            // Table cells never take the body first-line indent (the cell is its
            // own context). Without this they inherit the Normal style's indent.
            suppress_table_cell_indents(t);
        }
    }
}

/// Suppress the first-line indent on every paragraph inside a table's cells,
/// recursing into nested tables.
pub(super) fn suppress_table_cell_indents(table: &mut Table) {
    for row in &mut table.rows {
        for cell in &mut row.cells {
            for para in &mut cell.paragraphs {
                para.suppress_indent = true;
            }
            for content in &mut cell.content {
                match content {
                    CellContent::Paragraph(p) => p.suppress_indent = true,
                    CellContent::Table(nested) => suppress_table_cell_indents(nested),
                }
            }
        }
    }
}
pub(super) fn extract_document_metadata(html_doc: &HtmlDocument, doc: &mut Document) {
    // The `info` field is now private; read it through the `Document` trait's
    // `info()` accessor. The trait is referenced fully-qualified to avoid a name
    // clash with the OOXML `Document` already in scope (the `doc` parameter).
    let info = typst_library::model::Document::info(html_doc);
    // Prefer explicit metadata from `#set document(title: ..., author: ...)`
    if let Some(title) = &info.title {
        doc.metadata.title = Some(title.to_string());
    }
    if !info.author.is_empty() {
        doc.metadata.author = Some(
            info.author
                .iter()
                .map(typst::ecow::EcoString::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    // Fall back to first heading for title if not set via `#set document(title: ...)`
    if doc.metadata.title.is_none() {
        for elem in &doc.body.elements {
            if let BlockElement::Paragraph(p) = elem
                && matches!(p.style, Some(ParagraphStyle::Heading(_)))
            {
                let title = p.text_content();
                if !title.is_empty() {
                    doc.metadata.title = Some(title);
                }
                break;
            }
        }
    }
}
