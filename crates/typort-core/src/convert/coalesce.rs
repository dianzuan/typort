//! Run-coalescing post-pass.
//!
//! The HTML walk emits one `Run` per Typst text node (see
//! `convert/mod.rs::collect_par_inlines`), so a single logical line is shattered
//! into many tiny `<w:r>` runs and whitespace-only runs stay isolated. Nothing
//! else in the pipeline merges them. This pass runs LAST — after every per-run
//! style patch (`page::apply_styles_from_paged`, smallcaps, etc.) has settled —
//! and, for each `Paragraph` reachable in the document, merges adjacent
//! `InlineElement::Text` runs whose effective formatting is identical,
//! concatenating their text.
//!
//! Only plain `Text` runs merge. Any other inline (footnote ref, math, image,
//! bookmark, cross-reference, hyperlink, break, tab, citation) is an opaque
//! boundary the merge never crosses, so hyperlink / bookmark / footnote / math /
//! drawing structure is preserved bit-for-bit.

use typort_ooxml::document::{
    BlockElement, CellContent, Document, InlineElement, Paragraph, Run, Table,
};

/// Merge adjacent equally-formatted text runs across the whole document.
///
/// Covers the body (paragraphs, table cells including nested tables, and
/// bibliography blocks), every footnote body, and the header/footer.
pub fn coalesce_runs(doc: &mut Document) {
    for element in &mut doc.body.elements {
        coalesce_block(element);
    }

    for footnote in &mut doc.footnotes {
        coalesce_inlines(&mut footnote.content);
    }

    if let Some(header) = doc.header.as_mut() {
        for para in &mut header.paragraphs {
            coalesce_paragraph(para);
        }
    }
    if let Some(footer) = doc.footer.as_mut() {
        for para in &mut footer.paragraphs {
            coalesce_paragraph(para);
        }
    }
}

fn coalesce_block(element: &mut BlockElement) {
    match element {
        BlockElement::Paragraph(para) => coalesce_paragraph(para),
        BlockElement::Table(table) => coalesce_table(table),
        BlockElement::BibliographyBlock { paragraphs } => {
            for para in paragraphs {
                coalesce_paragraph(para);
            }
        }
    }
}

fn coalesce_table(table: &mut Table) {
    for row in &mut table.rows {
        for cell in &mut row.cells {
            for para in &mut cell.paragraphs {
                coalesce_paragraph(para);
            }
            for content in &mut cell.content {
                match content {
                    CellContent::Paragraph(para) => coalesce_paragraph(para),
                    CellContent::Table(nested) => coalesce_table(nested),
                }
            }
        }
    }
}

fn coalesce_paragraph(para: &mut Paragraph) {
    coalesce_inlines(&mut para.inlines);
}

/// Merge adjacent `Text` runs in an inline sequence, then fold lone
/// whitespace-only runs into a styled neighbour where it is visually safe.
/// Also recurses into hyperlink run vectors.
fn coalesce_inlines(inlines: &mut Vec<InlineElement>) {
    if inlines.len() > 1 {
        let mut merged: Vec<InlineElement> = Vec::with_capacity(inlines.len());
        for inline in inlines.drain(..) {
            match inline {
                InlineElement::Text(run) => {
                    if let Some(InlineElement::Text(prev)) = merged.last_mut()
                        && same_run_formatting(prev, &run)
                    {
                        prev.text.push_str(&run.text);
                        continue;
                    }
                    merged.push(InlineElement::Text(run));
                }
                other => merged.push(other),
            }
        }
        fold_whitespace_runs(&mut merged);
        // Drop any now-empty text runs left behind by folding a space into a
        // neighbour.
        merged.retain(|inline| !matches!(inline, InlineElement::Text(run) if run.text.is_empty()));
        *inlines = merged;
    }

    // Hyperlink display text is its own run vector; coalesce it in place. The
    // hyperlink is a single opaque inline above, so a run is never merged across
    // the hyperlink boundary.
    for inline in inlines.iter_mut() {
        if let InlineElement::Hyperlink { runs, .. } = inline {
            coalesce_run_vec(runs);
        }
    }
}

/// Coalesce a bare `Vec<Run>` (e.g. a hyperlink's display runs).
fn coalesce_run_vec(runs: &mut Vec<Run>) {
    if runs.len() < 2 {
        return;
    }
    let mut merged: Vec<Run> = Vec::with_capacity(runs.len());
    for run in runs.drain(..) {
        if let Some(prev) = merged.last_mut()
            && same_run_formatting(prev, &run)
        {
            prev.text.push_str(&run.text);
            continue;
        }
        merged.push(run);
    }
    *runs = merged;
}

/// Two runs have identical effective formatting iff every field the writer
/// turns into `<w:rPr>` matches. `text` and the non-serialized `span` are
/// deliberately ignored (the surviving run keeps the first run's span).
///
/// Mirrors the `has_rpr` field set in `typort_ooxml::writer::write_run` exactly;
/// if a new styled field is added there, add it here too.
fn same_run_formatting(a: &Run, b: &Run) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.superscript == b.superscript
        && a.subscript == b.subscript
        && a.monospace == b.monospace
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
        && a.highlight_color == b.highlight_color
        && a.smallcaps == b.smallcaps
        && a.color == b.color
        && a.font_ascii == b.font_ascii
        && a.font_east_asia == b.font_east_asia
        && a.size_half_pt == b.size_half_pt
}

/// A run whose text is non-empty and entirely whitespace.
fn is_whitespace_run(run: &Run) -> bool {
    !run.text.is_empty() && run.text.chars().all(char::is_whitespace)
}

/// Whether a run carries styling that is visible even on a bare space. A
/// highlight fill, an underline, and a strikethrough all draw across a space's
/// advance width (overline maps to `underline` in this model too), so a space
/// either carrying or adopting any of these is *not* invisible. Colour, font,
/// size and smallcaps need a glyph and so render nothing on whitespace.
fn renders_on_whitespace(run: &Run) -> bool {
    run.highlight_color.is_some() || run.underline || run.strikethrough
}

/// Fold an isolated whitespace-only run into an adjacent text run when doing so
/// changes nothing visible. Colour, font, size and smallcaps need a glyph, so a
/// bare space sitting beside text styled only in those ways may safely adopt the
/// neighbour's style and merge — collapsing the third run the shattering leaves
/// behind. We never fold when a *highlight*, *underline* or *strikethrough* is
/// involved (on the space itself or on the destination run), because those DO
/// draw across a space and folding would extend or erase a visible line. We only
/// ever pull the space INTO a neighbour, never rewrite a neighbour's style;
/// folded space runs are left empty for the caller to drop.
fn fold_whitespace_runs(inlines: &mut [InlineElement]) {
    let mut i = 0;
    while i < inlines.len() {
        // The space run must itself carry no visible-on-whitespace styling.
        let space_is_plain = matches!(
            &inlines[i],
            InlineElement::Text(run)
                if is_whitespace_run(run) && !renders_on_whitespace(run)
        );
        if !space_is_plain {
            i += 1;
            continue;
        }

        // Prefer folding into the PREVIOUS text run if present.
        if i > 0
            && let InlineElement::Text(prev) = &inlines[i - 1]
            && !renders_on_whitespace(prev)
        {
            let space_text = run_text(&inlines[i]);
            if let InlineElement::Text(prev_mut) = &mut inlines[i - 1] {
                prev_mut.text.push_str(&space_text);
            }
            set_run_text_empty(&mut inlines[i]);
            i += 1;
            continue;
        }

        // Otherwise fold into the NEXT text run.
        if i + 1 < inlines.len()
            && let InlineElement::Text(next) = &inlines[i + 1]
            && !renders_on_whitespace(next)
        {
            let space_text = run_text(&inlines[i]);
            if let InlineElement::Text(next_mut) = &mut inlines[i + 1] {
                next_mut.text.insert_str(0, &space_text);
            }
            set_run_text_empty(&mut inlines[i]);
        }
        i += 1;
    }
}

fn run_text(inline: &InlineElement) -> String {
    match inline {
        InlineElement::Text(run) => run.text.clone(),
        _ => String::new(),
    }
}

fn set_run_text_empty(inline: &mut InlineElement) {
    if let InlineElement::Text(run) = inline {
        run.text.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typort_ooxml::document::Run;

    fn text_run(s: &str) -> InlineElement {
        InlineElement::Text(Run::new(s))
    }

    fn bold_run(s: &str) -> InlineElement {
        let mut r = Run::new(s);
        r.bold = true;
        InlineElement::Text(r)
    }

    fn run_count(p: &Paragraph) -> usize {
        p.inlines
            .iter()
            .filter(|i| matches!(i, InlineElement::Text(_)))
            .count()
    }

    #[test]
    fn merges_adjacent_equal_runs() {
        let mut p = Paragraph::new();
        p.inlines = vec![text_run("Hello"), text_run(" "), text_run("world")];
        coalesce_paragraph(&mut p);
        assert_eq!(run_count(&p), 1);
        assert_eq!(p.text_content(), "Hello world");
    }

    #[test]
    fn preserves_styled_boundary() {
        let mut p = Paragraph::new();
        // plain | space | BOLD | space | plain
        p.inlines = vec![
            text_run("a"),
            text_run(" "),
            bold_run("b"),
            text_run(" "),
            text_run("c"),
        ];
        coalesce_paragraph(&mut p);
        // The bold span must remain its own run; the surrounding plain spaces
        // merge into the plain neighbours, collapsing to: "a " | "b" | " c".
        assert_eq!(run_count(&p), 3);
        assert_eq!(p.text_content(), "a b c");
        let bold_runs: Vec<&Run> = p.text_runs().filter(|r| r.bold).collect();
        assert_eq!(bold_runs.len(), 1);
        assert_eq!(bold_runs[0].text, "b");
    }

    #[test]
    fn does_not_cross_footnote_ref() {
        let mut p = Paragraph::new();
        p.inlines = vec![
            text_run("see"),
            InlineElement::FootnoteRef(2),
            text_run("here"),
        ];
        coalesce_paragraph(&mut p);
        assert_eq!(run_count(&p), 2);
        assert!(matches!(p.inlines[1], InlineElement::FootnoteRef(2)));
    }

    #[test]
    fn highlighted_space_not_folded() {
        let mut p = Paragraph::new();
        let mut sp = Run::new(" ");
        sp.highlight_color = Some("yellow".to_string());
        p.inlines = vec![text_run("a"), InlineElement::Text(sp), text_run("b")];
        coalesce_paragraph(&mut p);
        // A highlighted space is visible — it must survive as its own run.
        assert_eq!(run_count(&p), 3);
    }

    #[test]
    fn space_not_folded_into_underlined_or_struck_neighbour() {
        // `#underline[B] #underline[C]` arrives as B(u) | " "(plain) | C(u). The
        // plain space must NOT be folded into an underlined neighbour, or the
        // underline would extend continuously across the gap. Same for strike.
        for decorate in [
            |r: &mut Run| r.underline = true,
            |r: &mut Run| r.strikethrough = true,
        ] {
            let mut p = Paragraph::new();
            let (mut b, mut c) = (Run::new("B"), Run::new("C"));
            decorate(&mut b);
            decorate(&mut c);
            p.inlines = vec![
                InlineElement::Text(b),
                text_run(" "),
                InlineElement::Text(c),
            ];
            coalesce_paragraph(&mut p);
            assert_eq!(run_count(&p), 3, "decorated spans + the gap stay 3 runs");
            let spaces: Vec<&Run> = p.text_runs().filter(|r| r.text == " ").collect();
            assert_eq!(
                spaces.len(),
                1,
                "the inter-word space survives as its own run"
            );
            assert!(
                !spaces[0].underline && !spaces[0].strikethrough,
                "the space must not inherit the decoration"
            );
        }
    }
}
