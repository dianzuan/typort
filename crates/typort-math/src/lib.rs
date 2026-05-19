#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use quick_xml::Writer;
use quick_xml::events::BytesText;
use typst::foundations::Content;
use typst_library::foundations::{SequenceElem, SymbolElem};
use typst_library::math::{
    AccentElem, AlignPointElem, AttachElem, CasesElem, EquationElem, FracElem, LrElem, MatElem,
    OpElem, OverbraceElem, OverbracketElem, OverlineElem, OverparenElem, OvershellElem, RootElem,
    UnderbraceElem, UnderbracketElem, UnderlineElem, UnderparenElem, UndershellElem, VecElem,
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
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);

    if is_block {
        writer
            .create_element("m:oMathPara")
            .write_inner_content(|w| {
                write_omath(w, body)?;
                Ok(())
            })
            .expect("XML write failed");
    } else {
        write_omath(&mut writer, body).expect("XML write failed");
    }

    String::from_utf8(buf).expect("valid UTF-8")
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
        for child in &seq.children {
            convert_content(writer, child)?;
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
    } else if let Some(ob) = content.to_packed::<OverbraceElem>() {
        let ann = ob.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &ob.body, ann, "\u{23DE}", "top")?;
    } else if let Some(ub) = content.to_packed::<UnderbraceElem>() {
        let ann = ub.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &ub.body, ann, "\u{23DF}", "bot")?;
    } else if let Some(ob) = content.to_packed::<OverbracketElem>() {
        let ann = ob.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &ob.body, ann, "\u{23B4}", "top")?;
    } else if let Some(ub) = content.to_packed::<UnderbracketElem>() {
        let ann = ub.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &ub.body, ann, "\u{23B5}", "bot")?;
    } else if let Some(op) = content.to_packed::<OverparenElem>() {
        let ann = op.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &op.body, ann, "\u{23DC}", "top")?;
    } else if let Some(up) = content.to_packed::<UnderparenElem>() {
        let ann = up.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &up.body, ann, "\u{23DD}", "bot")?;
    } else if let Some(os) = content.to_packed::<OvershellElem>() {
        let ann = os.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &os.body, ann, "\u{23E0}", "top")?;
    } else if let Some(us) = content.to_packed::<UndershellElem>() {
        let ann = us.annotation.as_option().as_ref().and_then(|v| v.as_ref());
        convert_groupchr(writer, &us.body, ann, "\u{23E1}", "bot")?;
    } else if content.to_packed::<AlignPointElem>().is_some() {
        // Alignment points inside equation arrays are handled by the parent;
        // standalone occurrences are skipped (no OMML equivalent).
    } else if let Some(sym) = content.to_packed::<SymbolElem>() {
        write_math_run(writer, &sym.text)?;
    } else if let Some(text) = content.to_packed::<TextElem>() {
        write_math_run(writer, &text.text)?;
    } else if content.to_packed::<SpaceElem>().is_some() {
        // OMML handles inter-element spacing automatically — don't emit explicit spaces
    } else {
        // For unknown elements, skip silently (styled wrappers, etc.)
    }
    Ok(())
}

fn convert_attach<W: Write>(writer: &mut Writer<W>, attach: &AttachElem) -> std::io::Result<()> {
    let base = &attach.base;
    // t and b are Settable<Option<Content>>, as_option() -> &Option<Option<Content>>
    let sup = attach.t.as_option().as_ref().and_then(|v| v.as_ref());
    let sub = attach.b.as_option().as_ref().and_then(|v| v.as_ref());

    // Check if this is a nary operator (sum, integral, product, etc.)
    if is_nary_base(base) {
        return convert_nary(writer, base, sub, sup);
    }

    match (sub, sup) {
        (Some(below), Some(above)) => {
            // Both sub and super: m:sSubSup
            writer
                .create_element("m:sSubSup")
                .write_inner_content(|w| {
                    w.create_element("m:e").write_inner_content(|e| {
                        convert_content(e, base)?;
                        Ok(())
                    })?;
                    w.create_element("m:sub").write_inner_content(|s| {
                        convert_content(s, below)?;
                        Ok(())
                    })?;
                    w.create_element("m:sup").write_inner_content(|s| {
                        convert_content(s, above)?;
                        Ok(())
                    })?;
                    Ok(())
                })?;
        }
        (None, Some(above)) => {
            // Superscript only: m:sSup
            writer.create_element("m:sSup").write_inner_content(|w| {
                w.create_element("m:e").write_inner_content(|e| {
                    convert_content(e, base)?;
                    Ok(())
                })?;
                w.create_element("m:sup").write_inner_content(|s| {
                    convert_content(s, above)?;
                    Ok(())
                })?;
                Ok(())
            })?;
        }
        (Some(below), None) => {
            // Subscript only: m:sSub
            writer.create_element("m:sSub").write_inner_content(|w| {
                w.create_element("m:e").write_inner_content(|e| {
                    convert_content(e, base)?;
                    Ok(())
                })?;
                w.create_element("m:sub").write_inner_content(|s| {
                    convert_content(s, below)?;
                    Ok(())
                })?;
                Ok(())
            })?;
        }
        (None, None) => {
            // No scripts, just render the base
            convert_content(writer, base)?;
        }
    }
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

/// Convert a nary (big operator) with sub/superscripts to m:nary
fn convert_nary<W: Write>(
    writer: &mut Writer<W>,
    base: &Content,
    sub: Option<&Content>,
    sup: Option<&Content>,
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
        // The nary body in OMML wraps subsequent content; in Typst,
        // the following content is separate in the sequence. Leave body empty.
        w.create_element("m:e").write_inner_content(|_| Ok(()))?;
        Ok(())
    })?;
    Ok(())
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

    writer.create_element("m:d").write_inner_content(|w| {
        w.create_element("m:dPr").write_inner_content(|pr| {
            pr.create_element("m:begChr")
                .with_attribute(("m:val", open.as_str()))
                .write_empty()?;
            pr.create_element("m:endChr")
                .with_attribute(("m:val", close.as_str()))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
            for item in &inner {
                convert_content(e, item)?;
            }
            Ok(())
        })?;
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

    writer.create_element("m:d").write_inner_content(|w| {
        w.create_element("m:dPr").write_inner_content(|pr| {
            pr.create_element("m:begChr")
                .with_attribute(("m:val", open.as_str()))
                .write_empty()?;
            pr.create_element("m:endChr")
                .with_attribute(("m:val", close.as_str()))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
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
        })?;
        Ok(())
    })?;
    Ok(())
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

    writer.create_element("m:d").write_inner_content(|w| {
        w.create_element("m:dPr").write_inner_content(|pr| {
            pr.create_element("m:begChr")
                .with_attribute(("m:val", open.as_str()))
                .write_empty()?;
            pr.create_element("m:endChr")
                .with_attribute(("m:val", close.as_str()))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
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
        })?;
        Ok(())
    })?;
    Ok(())
}

/// Convert an `AccentElem` to `m:acc` with the appropriate combining character.
fn convert_accent<W: Write>(writer: &mut Writer<W>, accent: &AccentElem) -> std::io::Result<()> {
    // Map the Typst Accent to the OMML combining character.
    // OMML expects the combining Unicode character for the accent.
    let chr = accent_to_omml_char(accent.accent.0);

    writer.create_element("m:acc").write_inner_content(|w| {
        w.create_element("m:accPr").write_inner_content(|pr| {
            pr.create_element("m:chr")
                .with_attribute(("m:val", chr))
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

/// Map a Typst accent character to the OMML accent character string.
///
/// Typst normalizes accents to their combining Unicode form. OMML also
/// expects combining characters, so in most cases the character is used
/// directly. We handle the common cases explicitly to ensure correctness.
fn accent_to_omml_char(c: char) -> &'static str {
    match c {
        '\u{0303}' => "\u{0303}", // tilde
        '\u{20D7}' => "\u{20D7}", // combining right arrow above (vec)
        '\u{0307}' => "\u{0307}", // dot above
        '\u{0308}' => "\u{0308}", // diaeresis / double dot
        '\u{0300}' => "\u{0300}", // grave
        '\u{0301}' => "\u{0301}", // acute
        '\u{0304}' => "\u{0304}", // macron
        '\u{0305}' => "\u{0305}", // overline / dash
        '\u{0306}' => "\u{0306}", // breve
        '\u{030A}' => "\u{030A}", // ring above
        '\u{030C}' => "\u{030C}", // caron / háček
        '\u{20DB}' => "\u{20DB}", // triple dot
        '\u{20DC}' => "\u{20DC}", // quad dot
        '\u{030B}' => "\u{030B}", // double acute
        '\u{20D6}' => "\u{20D6}", // left arrow
        '\u{20E1}' => "\u{20E1}", // left-right arrow
        '\u{20D0}' => "\u{20D0}", // left harpoon
        '\u{20D1}' => "\u{20D1}", // right harpoon
        // Default: use combining circumflex (U+0302) as fallback — also covers hat
        _ => "\u{0302}",
    }
}

/// Convert `OverlineElem`/`UnderlineElem` to `m:bar` with position top/bot.
fn convert_bar<W: Write>(
    writer: &mut Writer<W>,
    body: &Content,
    pos: &str,
) -> std::io::Result<()> {
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
    let effective_close = if is_reverse {
        close_str.as_str()
    } else {
        ""
    };
    let effective_open = if is_reverse {
        ""
    } else {
        open_str.as_str()
    };

    writer.create_element("m:d").write_inner_content(|w| {
        w.create_element("m:dPr").write_inner_content(|pr| {
            pr.create_element("m:begChr")
                .with_attribute(("m:val", effective_open))
                .write_empty()?;
            pr.create_element("m:endChr")
                .with_attribute(("m:val", effective_close))
                .write_empty()?;
            Ok(())
        })?;
        w.create_element("m:e").write_inner_content(|e| {
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
        })?;
        Ok(())
    })?;
    Ok(())
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
        w.create_element("m:groupChr")
            .write_inner_content(|gc| {
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
            writer
                .create_element("m:limLow")
                .write_inner_content(|w| {
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
            writer
                .create_element("m:limUpp")
                .write_inner_content(|w| {
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
        let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
        write_math_run(&mut writer, "x").unwrap();
        let result = String::from_utf8(buf).unwrap();
        assert!(result.contains("<m:r>"));
        assert!(result.contains("<m:t>x</m:t>"));
        assert!(result.contains("</m:r>"));
    }

    #[test]
    fn test_write_math_run_empty_string() {
        let mut buf = Vec::new();
        let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
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
        let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
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
        let mut writer = Writer::new_with_indent(&mut buf, b' ', 2);
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
