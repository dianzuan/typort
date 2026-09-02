#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use codex::styling::{MathStyle, MathVariant, to_style};
use quick_xml::Writer;
use quick_xml::events::BytesText;
use typst::foundations::Content;
use typst_library::foundations::{SequenceElem, StyleChain, StyledElem, SymbolElem};
use typst_library::math::{
    AccentElem, AlignPointElem, AttachElem, CasesElem, ClassElem, EquationElem, FracElem, LrElem,
    MatElem, OpElem, OverbraceElem, OverbracketElem, OverlineElem, OverparenElem, OvershellElem,
    RootElem, UnderbraceElem, UnderbracketElem, UnderlineElem, UnderparenElem, UndershellElem,
    VecElem,
};
use typst_library::text::{LinebreakElem, SpaceElem, TextElem};

/// Convert a Typst `EquationElem` Content into an OMML XML string.
///
/// Returns the complete `<m:oMath>` (inline) or `<m:oMathPara><m:oMath>` (block) element.
///
/// # Panics
/// Panics if the content is not an `EquationElem`.
#[must_use]
pub fn equation_to_omml(content: &Content) -> String {
    let eq = content
        .to_packed::<EquationElem>()
        .expect("content must be an EquationElem");

    // block field: Settable<bool>, as_option() -> &Option<bool>
    let is_block = *eq.block.as_option().as_ref().unwrap_or(&false);
    let body = &eq.body;

    let mut buf = Vec::new();
    let mut writer = Writer::new(&mut buf);

    if is_block {
        expect_in_memory_xml_write(
            writer
                .create_element("m:oMathPara")
                .write_inner_content(|w| {
                    write_omath(w, body)?;
                    Ok(())
                }),
        );
    } else {
        expect_in_memory_xml_write(write_omath(&mut writer, body));
    }

    String::from_utf8(buf).expect("valid UTF-8")
}

/// In-memory XML writes target a `Vec<u8>`, whose `Write` implementation cannot fail.
fn expect_in_memory_xml_write<T>(result: std::io::Result<T>) {
    result.expect("writing XML to an in-memory buffer cannot fail");
}

fn write_omath<W: Write>(writer: &mut Writer<W>, body: &Content) -> std::io::Result<()> {
    writer.create_element("m:oMath").write_inner_content(|w| {
        // Check if the body is a multi-line aligned equation (has AlignPointElem + LinebreakElem)
        if is_aligned_equation(body) {
            convert_eq_array(w, body)?;
        } else {
            convert_content(w, body)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Check if a content tree represents a multi-line aligned equation.
///
/// Returns `true` when the top-level sequence contains both `AlignPointElem`
/// (alignment `&`) and `LinebreakElem` (line break `\`).
fn is_aligned_equation(content: &Content) -> bool {
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        let has_align = seq
            .children
            .iter()
            .any(|c| c.to_packed::<AlignPointElem>().is_some());
        let has_linebreak = seq
            .children
            .iter()
            .any(|c| c.to_packed::<LinebreakElem>().is_some());
        has_align && has_linebreak
    } else {
        false
    }
}

/// Convert a multi-line aligned equation to `m:eqArr`.
///
/// The body is split at `LinebreakElem` boundaries into rows. Each row
/// becomes an `<m:e>` inside the equation array. Within each row,
/// `AlignPointElem` is emitted as an ampersand `&` character which OMML
/// uses as an alignment tab stop inside `eqArr`.
fn convert_eq_array<W: Write>(writer: &mut Writer<W>, body: &Content) -> std::io::Result<()> {
    let seq = body
        .to_packed::<SequenceElem>()
        .expect("aligned equation body must be a SequenceElem");

    // Split children at LinebreakElem boundaries
    let mut rows: Vec<Vec<&Content>> = vec![Vec::new()];
    for child in &seq.children {
        if child.to_packed::<LinebreakElem>().is_some() {
            rows.push(Vec::new());
        } else {
            rows.last_mut().unwrap().push(child);
        }
    }

    // Remove trailing empty rows (can happen if there's a trailing linebreak)
    while rows.last().is_some_and(Vec::is_empty) {
        rows.pop();
    }

    writer
        .create_element("m:eqArr")
        .write_inner_content(|arr| {
            for row in &rows {
                arr.create_element("m:e").write_inner_content(|e| {
                    for child in row {
                        if child.to_packed::<AlignPointElem>().is_some() {
                            // Emit ampersand as OMML alignment tab inside eqArr
                            write_math_run(e, "\u{0026}")?;
                        } else {
                            convert_content(e, child)?;
                        }
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
    Ok(())
}

fn convert_content<W: Write>(writer: &mut Writer<W>, content: &Content) -> std::io::Result<()> {
    // Check what type the content is and dispatch accordingly
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        // Walk siblings with lookahead so a big operator (∫, ∑, …) can claim its
        // operand. Typst stores the operand as flat following siblings, but OMML's
        // m:nary needs it inside <m:e>. We consume forward until a Relation-class
        // symbol (=, <, ≤, →, …) or the end of the run — the operand boundary in
        // standard math notation. See convert_nary / is_relation_boundary.
        let children = &seq.children;
        let mut i = 0;
        while i < children.len() {
            if let Some((base, sub, sup)) = nary_attach_parts(&children[i]) {
                let mut end = i + 1;
                while end < children.len() && !is_relation_boundary(&children[end]) {
                    end += 1;
                }
                convert_nary(writer, base, sub, sup, &children[i + 1..end])?;
                i = end;
            } else {
                convert_content(writer, &children[i])?;
                i += 1;
            }
        }
    } else if let Some(attach) = content.to_packed::<AttachElem>() {
        convert_attach(writer, attach)?;
    } else if let Some(frac) = content.to_packed::<FracElem>() {
        convert_frac(writer, frac)?;
    } else if let Some(lr) = content.to_packed::<LrElem>() {
        convert_lr(writer, lr)?;
    } else if let Some(root) = content.to_packed::<RootElem>() {
        convert_root(writer, root)?;
    } else if let Some(mat) = content.to_packed::<MatElem>() {
        convert_mat(writer, mat)?;
    } else if let Some(vec_elem) = content.to_packed::<VecElem>() {
        convert_vec(writer, vec_elem)?;
    } else if let Some(accent) = content.to_packed::<AccentElem>() {
        convert_accent(writer, accent)?;
    } else if let Some(overline) = content.to_packed::<OverlineElem>() {
        convert_bar(writer, &overline.body, "top")?;
    } else if let Some(underline) = content.to_packed::<UnderlineElem>() {
        convert_bar(writer, &underline.body, "bot")?;
    } else if let Some(op) = content.to_packed::<OpElem>() {
        convert_op(writer, op)?;
    } else if let Some(cases) = content.to_packed::<CasesElem>() {
        convert_cases(writer, cases)?;
    } else if convert_group_character(writer, content)? {
    } else if content.to_packed::<AlignPointElem>().is_some() {
        // Alignment points inside equation arrays are handled by the parent;
        // standalone occurrences are skipped (no OMML equivalent).
    } else if let Some(class) = content.to_packed::<ClassElem>() {
        // A math-class wrapper. typst 0.15 wraps several constructs in a ClassElem
        // that 0.14 emitted bare — e.g. `dif` became `ClassElem(Unary, upright(d))`.
        // OMML has no class element (the class only tweaks spacing, which Word
        // derives itself), so emit the wrapped body. Without this arm the wrapped
        // glyph (e.g. the differential `d`) falls into the silent skip below.
        convert_content(writer, &class.body)?;
    } else if let Some(sym) = content.to_packed::<SymbolElem>() {
        write_math_run(writer, &sym.text)?;
    } else if let Some(text) = content.to_packed::<TextElem>() {
        write_math_run(writer, &text.text)?;
    } else if let Some(styled) = content.to_packed::<StyledElem>() {
        // Math style wrappers — bold(), bb(), cal(), upright(), dif, etc. — apply
        // an EquationElem variant/bold/italic to their child. Typst keeps the
        // inner SymbolElem unstyled and records the variant on this StyledElem, so
        // we resolve it here and re-emit the child's glyphs styled. Without this
        // the wrapper fell into the silent skip below, dropping the glyph entirely
        // (and leaving an empty <m:e> when it was a sub/superscript base).
        let chain = StyleChain::new(&styled.styles);
        convert_styled(
            writer,
            &styled.child,
            chain.get(EquationElem::variant),
            chain.get(EquationElem::bold),
            chain.get(EquationElem::italic),
        )?;
    } else if content.to_packed::<SpaceElem>().is_some() {
        // OMML handles inter-element spacing automatically — don't emit explicit spaces
    } else {
        // For unknown elements, skip silently.
    }
    Ok(())
}

/// Walk a styled math subtree, applying a resolved `(variant, bold, italic)` to
/// every text run it emits. Handles the nesting `bold(bb(x))` produces (each
/// wrapper sets one property) and falls back to plain [`convert_content`] for a
/// structural child (e.g. a fraction inside `bold(...)`) — rare, and that only
/// loses the wrapper styling inside the structure, never the content itself.
fn convert_styled<W: Write>(
    writer: &mut Writer<W>,
    content: &Content,
    variant: Option<MathVariant>,
    bold: bool,
    italic: Option<bool>,
) -> std::io::Result<()> {
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            convert_styled(writer, child, variant, bold, italic)?;
        }
    } else if let Some(styled) = content.to_packed::<StyledElem>() {
        // Nested wrapper: overlay its one property on the inherited style.
        let chain = StyleChain::new(&styled.styles);
        convert_styled(
            writer,
            &styled.child,
            chain.get(EquationElem::variant).or(variant),
            bold || chain.get(EquationElem::bold),
            chain.get(EquationElem::italic).or(italic),
        )?;
    } else if let Some(sym) = content.to_packed::<SymbolElem>() {
        write_styled_run(writer, &sym.text, variant, bold, italic)?;
    } else if let Some(text) = content.to_packed::<TextElem>() {
        write_styled_run(writer, &text.text, variant, bold, italic)?;
    } else if content.to_packed::<SpaceElem>().is_some() {
        // No explicit spaces in OMML.
    } else {
        convert_content(writer, content)?;
    }
    Ok(())
}

/// Emit a styled text run: map each char to its styled Unicode glyph via the
/// same `codex` logic Typst uses (`R` + double-struck -> ℝ, `e` + bold -> 𝒆).
/// An explicit upright request (`upright`/`dif`) leaves the glyph a plain ASCII
/// letter, which Word auto-italicizes, so that one case forces roman with an
/// `m:sty` of `"p"`; every other variant is encoded in the glyph itself.
fn write_styled_run<W: Write>(
    writer: &mut Writer<W>,
    text: &str,
    variant: Option<MathVariant>,
    bold: bool,
    italic: Option<bool>,
) -> std::io::Result<()> {
    let styled: String = text
        .chars()
        .flat_map(|c| to_style(c, MathStyle::select(c, variant, bold, italic)))
        .collect();
    let force_upright = italic == Some(false) && !bold;
    writer.create_element("m:r").write_inner_content(|w| {
        if force_upright {
            w.create_element("m:rPr").write_inner_content(|pr| {
                pr.create_element("m:sty")
                    .with_attribute(("m:val", "p"))
                    .write_empty()?;
                Ok(())
            })?;
        }
        w.create_element("m:t")
            .write_text_content(BytesText::new(&styled))?;
        Ok(())
    })?;
    Ok(())
}

fn convert_attach<W: Write>(writer: &mut Writer<W>, attach: &AttachElem) -> std::io::Result<()> {
    let base = &attach.base;
    // t and b are Settable<Option<Content>>, as_option() -> &Option<Option<Content>>
    let sup = attach.t.as_option().as_ref().and_then(|v| v.as_ref());
    let sub = attach.b.as_option().as_ref().and_then(|v| v.as_ref());

    // Check if this is a nary operator (sum, integral, product, etc.). When an
    // AttachElem reaches here directly (not via the sequence walk in
    // convert_content), there are no following siblings to bind, so the operand
    // is empty; the sequence walk is what supplies a real operand.
    if is_nary_base(base) {
        return convert_nary(writer, base, sub, sup, &[]);
    }

    if let Some(element) = script_attachment_element(sub.is_some(), sup.is_some()) {
        convert_script_attachment(writer, element, base, sub, sup)?;
    } else {
        convert_content(writer, base)?;
    }
    Ok(())
}

fn script_attachment_element(subscript: bool, superscript: bool) -> Option<&'static str> {
    const ATTACHMENTS: [((bool, bool), &str); 3] = [
        ((true, true), "m:sSubSup"),
        ((false, true), "m:sSup"),
        ((true, false), "m:sSub"),
    ];
    ATTACHMENTS
        .iter()
        .find_map(|&(scripts, element)| (scripts == (subscript, superscript)).then_some(element))
}

fn convert_script_attachment<W: Write>(
    writer: &mut Writer<W>,
    element: &str,
    base: &Content,
    sub: Option<&Content>,
    sup: Option<&Content>,
) -> std::io::Result<()> {
    writer.create_element(element).write_inner_content(|w| {
        w.create_element("m:e").write_inner_content(|e| {
            convert_content(e, base)?;
            Ok(())
        })?;
        if let Some(sub) = sub {
            w.create_element("m:sub").write_inner_content(|s| {
                convert_content(s, sub)?;
                Ok(())
            })?;
        }
        if let Some(sup) = sup {
            w.create_element("m:sup").write_inner_content(|s| {
                convert_content(s, sup)?;
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Check if the base of an `AttachElem` is a nary operator (sum, product, integral, etc.)
fn is_nary_base(content: &Content) -> bool {
    if let Some(sym) = content.to_packed::<SymbolElem>() {
        let text = sym.text.as_str();
        matches!(
            text,
            "\u{2211}" // summation
                | "\u{220F}" // product
                | "\u{222B}" // integral
                | "\u{222C}" // double integral
                | "\u{222D}" // triple integral
                | "\u{222E}" // contour integral
                | "\u{2210}" // coproduct
                | "\u{22C0}" // big wedge
                | "\u{22C1}" // big vee
                | "\u{22C2}" // big intersection
                | "\u{22C3}" // big union
        )
    } else {
        false
    }
}

/// Convert a nary (big operator) with sub/superscripts to m:nary.
///
/// `body` is the operand (integrand/summand) the caller collected from the
/// following siblings in the sequence — see [`convert_content`]. OMML requires
/// the operand inside `<m:e>`; Typst stores it flat after the operator, so the
/// caller binds it and passes it here. An empty `body` yields an empty `<m:e>`
/// (e.g. a bare `sum` with nothing after it).
fn convert_nary<W: Write>(
    writer: &mut Writer<W>,
    base: &Content,
    sub: Option<&Content>,
    sup: Option<&Content>,
    body: &[Content],
) -> std::io::Result<()> {
    let chr = if let Some(sym) = base.to_packed::<SymbolElem>() {
        sym.text.to_string()
    } else {
        "\u{2211}".to_string()
    };

    writer.create_element("m:nary").write_inner_content(|w| {
        w.create_element("m:naryPr").write_inner_content(|pr| {
            pr.create_element("m:chr")
                .with_attribute(("m:val", chr.as_str()))
                .write_empty()?;
            if sub.is_none() {
                pr.create_element("m:subHide")
                    .with_attribute(("m:val", "1"))
                    .write_empty()?;
            }
            if sup.is_none() {
                pr.create_element("m:supHide")
                    .with_attribute(("m:val", "1"))
                    .write_empty()?;
            }
            Ok(())
        })?;
        w.create_element("m:sub").write_inner_content(|s| {
            if let Some(sub_content) = sub {
                convert_content(s, sub_content)?;
            }
            Ok(())
        })?;
        w.create_element("m:sup").write_inner_content(|s| {
            if let Some(sup_content) = sup {
                convert_content(s, sup_content)?;
            }
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
            for item in body {
                convert_content(e, item)?;
            }
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

/// If `content` is an `AttachElem` whose base is a nary operator, return its
/// (base, sub, sup) so the caller can bind an operand and emit `m:nary`.
/// Also matches a bare nary symbol with no scripts.
fn nary_attach_parts(content: &Content) -> Option<(&Content, Option<&Content>, Option<&Content>)> {
    if let Some(attach) = content.to_packed::<AttachElem>() {
        if is_nary_base(&attach.base) {
            let sup = attach.t.as_option().as_ref().and_then(|v| v.as_ref());
            let sub = attach.b.as_option().as_ref().and_then(|v| v.as_ref());
            return Some((&attach.base, sub, sup));
        }
        return None;
    }
    if is_nary_base(content) {
        return Some((content, None, None));
    }
    None
}

/// Whether `content` is a symbol of the Relation math class (`=`, `<`, `≤`,
/// `→`, …). Such a symbol ends a nary operand: in `sum_i a_i = S`, the `= S`
/// is not part of the summand. Uses the same `unicode-math-class` table Typst
/// itself uses, so the boundary matches Typst's own classification.
fn is_relation_boundary(content: &Content) -> bool {
    let sym = content.to_packed::<SymbolElem>();
    let text = match sym {
        Some(s) => s.text.as_str(),
        None => return false,
    };
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        // Single-character symbol: classify it.
        (Some(c), None) => {
            matches!(
                unicode_math_class::class(c),
                Some(unicode_math_class::MathClass::Relation)
            )
        }
        _ => false,
    }
}

fn convert_frac<W: Write>(writer: &mut Writer<W>, frac: &FracElem) -> std::io::Result<()> {
    writer.create_element("m:f").write_inner_content(|w| {
        w.create_element("m:num").write_inner_content(|n| {
            convert_content(n, &frac.num)?;
            Ok(())
        })?;
        w.create_element("m:den").write_inner_content(|d| {
            convert_content(d, &frac.denom)?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

fn convert_lr<W: Write>(writer: &mut Writer<W>, lr: &LrElem) -> std::io::Result<()> {
    let body = &lr.body;

    // Extract delimiters and inner content from the body sequence
    let (open, close, inner) = extract_delimiters(body);

    write_delimited(writer, open.as_str(), close.as_str(), |e| {
        for item in &inner {
            convert_content(e, item)?;
        }
        Ok(())
    })
}

fn write_delimited<W: Write>(
    writer: &mut Writer<W>,
    open: &str,
    close: &str,
    write_body: impl FnOnce(&mut Writer<W>) -> std::io::Result<()>,
) -> std::io::Result<()> {
    writer.create_element("m:d").write_inner_content(|w| {
        w.create_element("m:dPr").write_inner_content(|pr| {
            pr.create_element("m:begChr")
                .with_attribute(("m:val", open))
                .write_empty()?;
            pr.create_element("m:endChr")
                .with_attribute(("m:val", close))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(write_body)?;
        Ok(())
    })?;
    Ok(())
}

/// Extract open/close delimiters and inner content from a `LrElem` body.
fn extract_delimiters(body: &Content) -> (String, String, Vec<&Content>) {
    let mut open = "(".to_string();
    let mut close = ")".to_string();
    let mut inner = Vec::new();

    if let Some(seq) = body.to_packed::<SequenceElem>() {
        let children = &seq.children;
        if children.is_empty() {
            return (open, close, inner);
        }

        // First child is the opening delimiter
        if let Some(sym) = children[0].to_packed::<SymbolElem>() {
            open = sym.text.to_string();
        }

        // Last child is the closing delimiter
        if children.len() > 1
            && let Some(sym) = children[children.len() - 1].to_packed::<SymbolElem>()
        {
            close = sym.text.to_string();
        }

        // Everything in between is the inner content
        if children.len() > 2 {
            for child in &children[1..children.len() - 1] {
                inner.push(child);
            }
        }
    } else {
        // Non-sequence body, just use as inner content
        inner.push(body);
    }

    (open, close, inner)
}

fn convert_root<W: Write>(writer: &mut Writer<W>, root: &RootElem) -> std::io::Result<()> {
    // index is Settable<Option<Content>>, as_option() -> &Option<Option<Content>>
    let index = root.index.as_option().as_ref().and_then(|v| v.as_ref());

    writer.create_element("m:rad").write_inner_content(|w| {
        w.create_element("m:radPr").write_inner_content(|pr| {
            if index.is_none() {
                pr.create_element("m:degHide")
                    .with_attribute(("m:val", "1"))
                    .write_empty()?;
            }
            Ok(())
        })?;
        w.create_element("m:deg").write_inner_content(|d| {
            if let Some(idx) = index {
                convert_content(d, idx)?;
            }
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
            convert_content(e, &root.radicand)?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

/// Convert a `MatElem` (matrix) to `m:m` with `m:mr` rows and `m:e` cells.
/// The matrix is wrapped in `m:d` delimiters matching the Typst delimiter pair.
fn convert_mat<W: Write>(writer: &mut Writer<W>, mat: &MatElem) -> std::io::Result<()> {
    // MatElem default delimiter is PAREN: ( )
    let (open, close) = if let Some(delim) = mat.delim.as_option().as_ref() {
        (
            delim.open().map_or_else(String::new, |c| c.to_string()),
            delim.close().map_or_else(String::new, |c| c.to_string()),
        )
    } else {
        ("(".to_string(), ")".to_string())
    };

    write_delimited(writer, open.as_str(), close.as_str(), |e| {
        e.create_element("m:m").write_inner_content(|m| {
            for row in &mat.rows {
                m.create_element("m:mr").write_inner_content(|mr| {
                    for cell in row {
                        mr.create_element("m:e").write_inner_content(|ce| {
                            convert_content(ce, cell)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        Ok(())
    })
}

/// Convert a `VecElem` (column vector) to `m:d` wrapping `m:m` with one column.
fn convert_vec<W: Write>(writer: &mut Writer<W>, vec_elem: &VecElem) -> std::io::Result<()> {
    // VecElem default delimiter is PAREN: ( )
    let (open, close) = if let Some(delim) = vec_elem.delim.as_option().as_ref() {
        (
            delim.open().map_or_else(String::new, |c| c.to_string()),
            delim.close().map_or_else(String::new, |c| c.to_string()),
        )
    } else {
        ("(".to_string(), ")".to_string())
    };

    write_delimited(writer, open.as_str(), close.as_str(), |e| {
        e.create_element("m:m").write_inner_content(|m| {
            for child in &vec_elem.children {
                m.create_element("m:mr").write_inner_content(|mr| {
                    mr.create_element("m:e").write_inner_content(|ce| {
                        convert_content(ce, child)?;
                        Ok(())
                    })?;
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        Ok(())
    })
}

/// Convert an `AccentElem` to `m:acc` with the appropriate combining character.
fn convert_accent<W: Write>(writer: &mut Writer<W>, accent: &AccentElem) -> std::io::Result<()> {
    // Typst and OMML both use the normalized combining Unicode character.
    let chr = accent.accent.0.to_string();

    writer.create_element("m:acc").write_inner_content(|w| {
        w.create_element("m:accPr").write_inner_content(|pr| {
            pr.create_element("m:chr")
                .with_attribute(("m:val", chr.as_str()))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
            convert_content(e, &accent.base)?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

/// Convert `OverlineElem`/`UnderlineElem` to `m:bar` with position top/bot.
fn convert_bar<W: Write>(writer: &mut Writer<W>, body: &Content, pos: &str) -> std::io::Result<()> {
    writer.create_element("m:bar").write_inner_content(|w| {
        w.create_element("m:barPr").write_inner_content(|pr| {
            pr.create_element("m:pos")
                .with_attribute(("m:val", pos))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
            convert_content(e, body)?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

/// Convert an `OpElem` (named math operator like sin, cos, lim) to `m:func`.
///
/// The function name is rendered in plain (upright) style via `m:sty m:val="p"`.
fn convert_op<W: Write>(writer: &mut Writer<W>, op: &OpElem) -> std::io::Result<()> {
    // Extract the operator text from the OpElem's text field.
    // OpElem.text is a Content that wraps a TextElem with the operator name.
    let op_text = extract_text_content(&op.text);

    writer.create_element("m:func").write_inner_content(|w| {
        w.create_element("m:fName").write_inner_content(|fname| {
            fname.create_element("m:r").write_inner_content(|r| {
                r.create_element("m:rPr").write_inner_content(|rpr| {
                    rpr.create_element("m:sty")
                        .with_attribute(("m:val", "p"))
                        .write_empty()?;
                    Ok(())
                })?;
                r.create_element("m:t")
                    .write_text_content(BytesText::new(&op_text))?;
                Ok(())
            })?;
            Ok(())
        })?;
        // OpElem in Typst is standalone — the argument is external in the
        // Content tree (attached via AttachElem or adjacent). Emit empty body.
        w.create_element("m:e").write_inner_content(|_| Ok(()))?;
        Ok(())
    })?;
    Ok(())
}

/// Recursively extract plain text from a Content tree.
fn extract_text_content(content: &Content) -> String {
    if let Some(text) = content.to_packed::<TextElem>() {
        return text.text.to_string();
    }
    if let Some(sym) = content.to_packed::<SymbolElem>() {
        return sym.text.to_string();
    }
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        let mut result = String::new();
        for child in &seq.children {
            result.push_str(&extract_text_content(child));
        }
        return result;
    }
    String::new()
}

/// Convert a `CasesElem` to `m:d` (left brace) wrapping `m:eqArr`.
fn convert_cases<W: Write>(writer: &mut Writer<W>, cases: &CasesElem) -> std::io::Result<()> {
    let is_reverse = *cases.reverse.as_option().as_ref().unwrap_or(&false);

    // CasesElem default delimiter is BRACE: { }
    let (delim_open, delim_close) = if let Some(delim) = cases.delim.as_option().as_ref() {
        (
            delim.open().map_or_else(String::new, |c| c.to_string()),
            delim.close().map_or_else(String::new, |c| c.to_string()),
        )
    } else {
        ("{".to_string(), "}".to_string())
    };

    let (open_str, close_str) = if is_reverse {
        (delim_close, delim_open)
    } else {
        (delim_open, delim_close)
    };

    // For standard (non-reverse) cases, suppress the closing delimiter
    let effective_close = if is_reverse { close_str.as_str() } else { "" };
    let effective_open = if is_reverse { "" } else { open_str.as_str() };

    write_delimited(writer, effective_open, effective_close, |e| {
        e.create_element("m:eqArr").write_inner_content(|arr| {
            for child in &cases.children {
                arr.create_element("m:e").write_inner_content(|ce| {
                    convert_content(ce, child)?;
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        Ok(())
    })
}

fn convert_group_character<W: Write>(
    writer: &mut Writer<W>,
    content: &Content,
) -> std::io::Result<bool> {
    macro_rules! group_characters {
        ($(($construct:ty, $character:literal, $position:literal)),+ $(,)?) => {
            $(
                if let Some(group) = content.to_packed::<$construct>() {
                    let annotation = group
                        .annotation
                        .as_option()
                        .as_ref()
                        .and_then(|value| value.as_ref());
                    convert_groupchr(
                        writer,
                        &group.body,
                        annotation,
                        $character,
                        $position,
                    )?;
                    return Ok(true);
                }
            )+
        };
    }

    group_characters![
        (OverbraceElem, "\u{23DE}", "top"),
        (UnderbraceElem, "\u{23DF}", "bot"),
        (OverbracketElem, "\u{23B4}", "top"),
        (UnderbracketElem, "\u{23B5}", "bot"),
        (OverparenElem, "\u{23DC}", "top"),
        (UnderparenElem, "\u{23DD}", "bot"),
        (OvershellElem, "\u{23E0}", "top"),
        (UndershellElem, "\u{23E1}", "bot"),
    ];
    Ok(false)
}

/// Convert overbrace/underbrace/overbracket/underbracket/overparen/underparen/
/// overshell/undershell to `m:groupChr`.
///
/// If there is an annotation, we wrap it with the group character using
/// `m:limLow` (for "bot" position) or `m:limUpp` (for "top" position) to
/// place the annotation below/above the group character structure.
fn convert_groupchr<W: Write>(
    writer: &mut Writer<W>,
    body: &Content,
    annotation: Option<&Content>,
    chr: &str,
    pos: &str,
) -> std::io::Result<()> {
    // The groupChr element itself
    let write_group = |w: &mut Writer<W>| -> std::io::Result<()> {
        w.create_element("m:groupChr").write_inner_content(|gc| {
            gc.create_element("m:groupChrPr")
                .write_inner_content(|pr| {
                    pr.create_element("m:chr")
                        .with_attribute(("m:val", chr))
                        .write_empty()?;
                    pr.create_element("m:pos")
                        .with_attribute(("m:val", pos))
                        .write_empty()?;
                    // vertJc controls where the character sits relative to the base
                    pr.create_element("m:vertJc")
                        .with_attribute(("m:val", pos))
                        .write_empty()?;
                    Ok(())
                })?;
            gc.create_element("m:e").write_inner_content(|e| {
                convert_content(e, body)?;
                Ok(())
            })?;
            Ok(())
        })?;
        Ok(())
    };

    if let Some(ann) = annotation {
        // Wrap in m:limLow (bottom annotation) or m:limUpp (top annotation)
        if pos == "bot" {
            writer.create_element("m:limLow").write_inner_content(|w| {
                w.create_element("m:e").write_inner_content(|e| {
                    write_group(e)?;
                    Ok(())
                })?;
                w.create_element("m:lim").write_inner_content(|lim| {
                    convert_content(lim, ann)?;
                    Ok(())
                })?;
                Ok(())
            })?;
        } else {
            writer.create_element("m:limUpp").write_inner_content(|w| {
                w.create_element("m:e").write_inner_content(|e| {
                    write_group(e)?;
                    Ok(())
                })?;
                w.create_element("m:lim").write_inner_content(|lim| {
                    convert_content(lim, ann)?;
                    Ok(())
                })?;
                Ok(())
            })?;
        }
    } else {
        write_group(writer)?;
    }
    Ok(())
}

fn write_math_run<W: Write>(writer: &mut Writer<W>, text: &str) -> std::io::Result<()> {
    writer.create_element("m:r").write_inner_content(|w| {
        w.create_element("m:t")
            .write_text_content(BytesText::new(text))?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_math_run() {
        let mut buf = Vec::new();
        let mut writer = Writer::new(&mut buf);
        write_math_run(&mut writer, "x").unwrap();
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("<m:r>"));
        assert!(result.contains("<m:t>x</m:t>"));
        assert!(result.contains("</m:r>"));
    }

    #[test]
    fn test_write_math_run_empty_string() {
        let mut buf = Vec::new();
        let mut writer = Writer::new(&mut buf);
        write_math_run(&mut writer, "").unwrap();
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("<m:r>"), "should still produce m:r element");
        assert!(
            result.contains("<m:t></m:t>") || result.contains("<m:t/>"),
            "should produce empty m:t element, got: {result}"
        );
    }

    #[test]
    fn test_write_math_run_unicode() {
        let mut buf = Vec::new();
        let mut writer = Writer::new(&mut buf);
        write_math_run(&mut writer, "\u{03B1}").unwrap(); // alpha
        let result = String::from_utf8(buf).unwrap();
        assert!(
            result.contains("<m:t>\u{03B1}</m:t>"),
            "should contain Unicode alpha character, got: {result}"
        );
    }

    #[test]
    fn test_write_math_run_xml_special_chars() {
        let mut buf = Vec::new();
        let mut writer = Writer::new(&mut buf);
        write_math_run(&mut writer, "&<>").unwrap();
        let result = String::from_utf8(buf).unwrap();
        assert!(
            !result.contains("<m:t>&<></m:t>"),
            "XML special chars should be escaped, got: {result}"
        );
        assert!(
            result.contains("&amp;"),
            "ampersand should be escaped to &amp;, got: {result}"
        );
        assert!(
            result.contains("&lt;"),
            "less-than should be escaped to &lt;, got: {result}"
        );
        assert!(
            result.contains("&gt;"),
            "greater-than should be escaped to &gt;, got: {result}"
        );
    }
}
