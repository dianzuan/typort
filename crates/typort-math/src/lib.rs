#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use quick_xml::Writer;
use quick_xml::events::BytesText;
use typst::foundations::Content;
use typst_library::foundations::{SequenceElem, SymbolElem};
use typst_library::math::{AttachElem, EquationElem, FracElem, LrElem, RootElem};
use typst_library::text::{SpaceElem, TextElem};

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
        convert_content(w, body)?;
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
}
