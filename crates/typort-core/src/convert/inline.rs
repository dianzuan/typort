//! Content AST -> Run extraction.
//!
//! Walks the Typst `Content` tree to produce a flat list of `Run` values
//! with bold / italic / etc. formatting flags.

use super::fmt::InlineFmt;
use typort_ooxml::document::Run;
use typst::foundations::Content;
use typst_library::foundations::{SequenceElem, SymbolElem};
use typst_library::model::{EmphElem, ParElem, StrongElem};
use typst_library::text::{LinebreakElem, SmallcapsElem, SpaceElem, SubElem, SuperElem, TextElem};

/// Extract a flat list of [`Run`] from a `Content` body.
///
/// This recursively walks `SequenceElem`, `StrongElem`, `EmphElem`,
/// `TextElem`, and `SpaceElem` nodes, collecting formatted runs.
#[must_use]
pub fn extract_runs(content: &Content) -> Vec<Run> {
    let mut runs = Vec::new();
    walk_content(content, InlineFmt::default(), &mut runs);
    runs
}

fn walk_content(content: &Content, fmt: InlineFmt, runs: &mut Vec<Run>) {
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            walk_content(child, fmt, runs);
        }
    } else if let Some(strong) = content.to_packed::<StrongElem>() {
        walk_content(&strong.body, fmt.for_tag("strong"), runs);
    } else if let Some(emph) = content.to_packed::<EmphElem>() {
        walk_content(&emph.body, fmt.for_tag("emph"), runs);
    } else if let Some(sc) = content.to_packed::<SmallcapsElem>() {
        walk_content(
            &sc.body,
            InlineFmt {
                smallcaps: true,
                ..fmt
            },
            runs,
        );
    } else if let Some(sup) = content.to_packed::<SuperElem>() {
        walk_content(
            &sup.body,
            InlineFmt {
                superscript: true,
                ..fmt
            },
            runs,
        );
    } else if let Some(sub) = content.to_packed::<SubElem>() {
        walk_content(
            &sub.body,
            InlineFmt {
                subscript: true,
                ..fmt
            },
            runs,
        );
    } else if let Some(par) = content.to_packed::<ParElem>() {
        walk_content(&par.body, fmt, runs);
    } else if let Some(text) = content.to_packed::<TextElem>() {
        let mut run = Run::new(text.text.as_str());
        fmt.apply_to(&mut run);
        let sp = content.span();
        if !sp.is_detached() {
            run.span = Some(sp);
        }
        runs.push(run);
    } else if let Some(sym) = content.to_packed::<SymbolElem>() {
        let mut run = Run::new(sym.text.as_str());
        fmt.apply_to(&mut run);
        let sp = content.span();
        if !sp.is_detached() {
            run.span = Some(sp);
        }
        runs.push(run);
    } else if content.to_packed::<LinebreakElem>().is_some() {
        // A forced line break (`\`): emit a real break run so the surrounding words
        // don't glue together.
        runs.push(Run::line_break());
    } else if content.to_packed::<SpaceElem>().is_some() {
        // Merge a space into the previous run when possible, otherwise emit a new run.
        if let Some(last) = runs.last_mut() {
            last.text.push(' ');
        } else {
            runs.push(Run::new(" "));
        }
    }
    // Unknown element types are silently skipped for now.
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::foundations::{Content, NativeElement};
    use typst_library::foundations::SequenceElem;
    use typst_library::text::TextElem;

    fn text(s: &str) -> Content {
        TextElem::new(s.into()).pack()
    }

    fn space() -> Content {
        SpaceElem::new().pack()
    }

    #[test]
    fn plain_text_single_run() {
        let c = text("hello");
        let runs = extract_runs(&c);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hello");
        assert!(!runs[0].bold);
        assert!(!runs[0].italic);
    }

    #[test]
    fn strong_sets_bold() {
        let c = StrongElem::new(text("bold")).pack();
        let runs = extract_runs(&c);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].bold);
        assert_eq!(runs[0].text, "bold");
    }

    #[test]
    fn emph_sets_italic() {
        let c = EmphElem::new(text("ital")).pack();
        let runs = extract_runs(&c);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].italic);
    }

    #[test]
    fn sequence_with_space() {
        let c = SequenceElem::new(vec![text("a"), space(), text("b")]).pack();
        let runs = extract_runs(&c);
        // "a" + space merged -> "a ", then "b"
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "a ");
        assert_eq!(runs[1].text, "b");
    }
}

#[cfg(test)]
mod sc_test {
    use super::*;
    use typst::foundations::{Content, NativeElement};

    #[test]
    fn smallcaps_detected_in_extract_runs() {
        let inner = Content::sequence(vec![TextElem::packed("hello")]);
        let sc = typst_library::text::SmallcapsElem::new(inner).pack();
        let runs = extract_runs(&sc);
        assert!(!runs.is_empty(), "should extract runs from SmallcapsElem");
        assert!(runs[0].smallcaps, "run should have smallcaps=true");
    }
}
