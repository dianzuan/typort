// ===========================================================================
// Golden snapshots: pin the exact `word/document.xml` for a curated fixture set
// so any output-formatting drift surfaces as a reviewable diff (the suite's only
// oracle for output *quality*, not mere presence). quick-xml already emits
// deterministic 2-space-indented XML (writer.rs `new_with_indent`), verified
// byte-identical across separate processes, so we snapshot it verbatim — no
// pretty-printer, no new dependency.
//
// CURATION — CI-safety: the World loads system fonts (world.rs
// `include_system_fonts(true)`). Declaring a CJK font in source pins the font
// NAME in the output (environment-independent), but it is NOT sufficient for a
// byte-exact snapshot: properties DETECTED from rendering — bold weight, size —
// still require that font to be INSTALLED on the runner. complex_paper declares
// "Noto Serif SC", yet CI (which installs no CJK fonts) substitutes it and no
// longer detects the author name as bold, so its snapshot flaked. The set below
// is therefore limited to fixtures whose fonts are embedded (Libertinus) or
// constant ("Courier New"). CJK fixtures — whether the font is declared or
// detected (complex_paper, hello, issue_cjk_heading_numbering,
// edge_three_line_table) — are excluded; they are covered by the substring-based
// tests above.
//
// Regenerate after an intentional change, then review the diff before committing:
//   UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden
//   git diff tests/snapshots
use crate::common::fixture_doc_xml;

/// Path of a committed golden, relative to the crate dir (where tests run),
/// mirroring the `../../tests/...` convention `fixture_doc_xml` uses.
fn golden_path(fixture: &str) -> std::path::PathBuf {
    std::path::Path::new("../../tests/snapshots").join(format!("{fixture}.document.xml"))
}

/// Normalize for comparison: strip trailing whitespace per line and force LF,
/// defending against CRLF checkouts and editor end-of-line churn.
fn normalize_xml(xml: &str) -> String {
    let body = xml.replace("\r\n", "\n");
    let mut out: String = body
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// First differing line, for a one-line failure message instead of a huge
/// dump. Returns `(line_no, expected, actual)`.
fn first_diff<'a>(expected: &'a str, actual: &'a str) -> Option<(usize, &'a str, &'a str)> {
    for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return Some((i + 1, e, a));
        }
    }
    let (el, al) = (expected.lines().count(), actual.lines().count());
    if el != al {
        return Some((
            el.min(al) + 1,
            "<line count differs>",
            "<line count differs>",
        ));
    }
    None
}

/// Convert the fixture, normalize, and either (re)write the golden
/// (`UPDATE_SNAPSHOTS=1`) or assert byte-equality against the committed one.
fn check_golden(fixture: &str) {
    let actual = normalize_xml(&fixture_doc_xml(fixture));
    let path = golden_path(fixture);

    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &actual)
            .unwrap_or_else(|e| panic!("failed to write golden {}: {e}", path.display()));
        return;
    }

    let expected = match std::fs::read_to_string(&path) {
        Ok(s) => normalize_xml(&s),
        Err(_) => panic!(
            "missing golden {}\nregenerate with: \
             UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden",
            path.display()
        ),
    };

    if let Some((line, e, a)) = first_diff(&expected, &actual) {
        panic!(
            "golden mismatch for {fixture} at line {line}\n  expected: {e}\n  actual:   {a}\n\
             \nif this change is intentional, regenerate and review:\n  \
             UPDATE_SNAPSHOTS=1 cargo test -p typort --test integration golden\n  \
             git diff tests/snapshots"
        );
    }
}

macro_rules! golden_test {
    ($name:ident, $fixture:literal) => {
        #[test]
        fn $name() {
            check_golden($fixture);
        }
    };
}

golden_test!(golden_aligned_equations, "aligned_equations");
golden_test!(golden_inline_math_in_text, "inline_math_in_text");
golden_test!(golden_edge_complex_table, "edge_complex_table");
golden_test!(golden_formatted_footnote, "formatted_footnote");
golden_test!(golden_edge_term_list, "edge_term_list");
golden_test!(golden_edge_deep_nested_list, "edge_deep_nested_list");
golden_test!(golden_bibliography_basic, "bibliography_basic");
golden_test!(golden_edge_theorem_proof, "edge_theorem_proof");
