//! CJK font handling and detection tests.

use crate::common::{fixture_doc_xml, paragraph_containing};

#[test]
fn issue_cjk_latin_font_mixing_content() {
    let xml = fixture_doc_xml("issue_cjk_latin_font_mixing");
    assert!(xml.contains("中文正文"), "CJK text should be present");
    assert!(xml.contains("English"), "Latin text should be present");
    assert!(xml.contains("2024"), "numbers should be present");
}

#[test]
fn issue_cjk_font_east_asia() {
    let path = "../../tests/fixtures/issue_cjk_font_east_asia.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(archive.by_name("word/document.xml").unwrap()).unwrap();
    let styles_xml = std::io::read_to_string(archive.by_name("word/styles.xml").unwrap()).unwrap();
    assert!(
        doc_xml.contains("eastAsia") || styles_xml.contains("eastAsia"),
        "should set w:rFonts eastAsia attribute for CJK (in document or styles)"
    );
    assert!(
        doc_xml.contains("\u{65E5}\u{672C}\u{8A9E}"),
        "Japanese text should be present"
    );
    assert!(
        doc_xml.contains("\u{4E2D}\u{6587}"),
        "Chinese text should be present"
    );
}

/// CJK fonts declared by English family name must reach Word as the localized
/// display name (`SimSun` → `宋体`, `SimHei` → `黑体`, `KaiTi` → `楷体`).
///
/// This translation reads each font's own name table, so it only fires when the
/// font is actually installed. CI installs no CJK fonts (see CLAUDE.md), so the
/// test gates each assertion on availability: with the font present it asserts
/// the localized name; absent, it asserts the documented no-op (the English name
/// is kept) and notes the skip. Both branches assert — it never silently passes.
#[test]
fn issue_cjk_font_localized_name() {
    use typst::World as _;

    let path = "../../tests/fixtures/issue_cjk_font_localized_name.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let installed = |family: &str| {
        world
            .book()
            .select_family(&family.to_lowercase())
            .next()
            .is_some()
    };
    let (simsun, simhei, kaiti) = (installed("SimSun"), installed("SimHei"), installed("KaiTi"));

    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(archive.by_name("word/document.xml").unwrap()).unwrap();
    let styles_xml = std::io::read_to_string(archive.by_name("word/styles.xml").unwrap()).unwrap();

    // Body default (styles.xml) — the bulk of the document inherits it.
    if simsun {
        assert!(
            styles_xml.contains(r#"w:eastAsia="宋体""#),
            "SimSun body default must localize to 宋体, got styles: {styles_xml}"
        );
        assert!(
            !styles_xml.contains(r#"w:eastAsia="SimSun""#),
            "no raw English SimSun should remain once localized"
        );
    } else {
        assert!(
            styles_xml.contains(r#"w:eastAsia="SimSun""#),
            "without SimSun installed the English name is kept (no-op); CI skip"
        );
        eprintln!("note: SimSun not installed — localized-name assertion skipped (expected on CI)");
    }

    // Per-run overrides (document.xml): heading 黑体, KaiTi span 楷体.
    if simhei {
        assert!(
            doc_xml.contains(r#"w:eastAsia="黑体""#),
            "SimHei heading run must localize to 黑体, got: {doc_xml}"
        );
    }
    if kaiti {
        assert!(
            doc_xml.contains(r#"w:eastAsia="楷体""#),
            "KaiTi span must localize to 楷体, got: {doc_xml}"
        );
    }
}

/// A CJK-only fallback font list `#set text(font: ("NSimSun", "Noto Serif SC"))`
/// must not let the never-rendered glyph-fallback (`Noto Serif SC`) become the
/// `w:eastAsia` body default. The positional "fonts[0]=Latin, fonts[1]=CJK" split
/// would pick `Noto Serif SC` (then localize it to the weight name
/// `Noto Serif SC Light`), clobbering the geometry-detected rendered CJK font
/// (`NSimSun`). The fix cross-checks the declared list against the detected fonts.
#[test]
fn issue_cjk_fallback_list_font() {
    use typst::World as _;

    let path = "../../tests/fixtures/issue_cjk_fallback_list_font.typ";
    let world = typort_core::TyportWorld::new(std::path::Path::new(path)).unwrap();
    let nsimsun_installed = world.book().select_family("nsimsun").next().is_some();

    let doc = typort_core::convert::convert(&world).unwrap();
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, std::io::Cursor::new(&mut buf)).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
    let styles_xml = std::io::read_to_string(archive.by_name("word/styles.xml").unwrap()).unwrap();

    // Font-INDEPENDENT key regression: the never-rendered fallback must never
    // reach eastAsia, in any form (raw or localized weight name). FAILS before
    // the fix (eastAsia="Noto Serif SC Light"), PASSES after.
    assert!(
        !styles_xml.contains("Noto Serif SC"),
        "the never-rendered CJK fallback must not become the eastAsia body default, got styles: {styles_xml}"
    );

    // Font-DEPENDENT: with NSimSun installed it localizes to 新宋体; on CI (no
    // CJK fonts) the raw declared primary is kept. Both branches assert.
    if nsimsun_installed {
        assert!(
            styles_xml.contains(r#"w:eastAsia="新宋体""#),
            "NSimSun body default must localize to 新宋体, got styles: {styles_xml}"
        );
    } else {
        assert!(
            styles_xml.contains(r#"w:eastAsia="NSimSun""#),
            "without NSimSun installed the raw primary is kept; CI skip"
        );
        eprintln!(
            "note: NSimSun not installed — localized-name assertion skipped (expected on CI)"
        );
    }
}

#[test]
fn issue_underline_font_size_change() {
    let xml = fixture_doc_xml("issue_underline_font_size_change");
    assert!(xml.contains("<w:u "), "should have underline formatting");
    assert!(
        xml.contains("w:strike"),
        "should have strikethrough formatting"
    );
    assert!(xml.contains("Normal size"), "underlined text present");
    assert!(xml.contains("Regular"), "strikethrough text present");
}

#[test]
fn no_math_fallback_font_on_plain_digit_or_whitespace() {
    // Regression: per-glyph math fallback must not leak a math font onto plain
    // text, and a whitespace-only run must not carry a stray size. Typst shapes
    // the isolated digit '7' in "[7]" with a math-table face; copying the paged
    // run style verbatim used to emit `w:rFonts w:ascii="...Math"` on that digit.
    // Detection now normalizes any FontFlags::MATH face (and any non-letter run
    // whose font differs from baseline) back to the baseline, and drops all
    // overrides on whitespace-only runs. See tests/fixtures/edge_math_fallback_digit.typ.
    let xml = fixture_doc_xml("edge_math_fallback_digit");

    // (a) No run carries a *Math* face on its rFonts (the OpenType math family
    //     name marker), anywhere in the document.
    for rfonts in xml.split("<w:rFonts").skip(1) {
        let tag = rfonts.split('>').next().unwrap_or("");
        assert!(
            !tag.contains("Math"),
            "no run should carry a math fallback font; found rFonts: <w:rFonts{tag}>"
        );
    }

    // (b) The bare digit '7' must still survive as body text.
    assert!(
        xml.contains(">7<"),
        "expected the digit 7 to survive as a run"
    );

    // (c) A whitespace-only run must not carry a size override.
    for run in xml.split("<w:r>").skip(1) {
        let body = run.split("</w:r>").next().unwrap_or("");
        let Some(after_t) = body.split("<w:t").nth(1) else {
            continue;
        };
        let text = after_t
            .split_once('>')
            .and_then(|(_, rest)| rest.split("</w:t>").next())
            .unwrap_or("");
        if !text.is_empty() && text.chars().all(char::is_whitespace) {
            assert!(
                !body.contains("<w:sz "),
                "whitespace-only run must inherit size, got run: {body}"
            );
        }
    }
}

#[test]
fn variable_font_weight_bold_detection() {
    // Variable fonts (typst 0.15) report continuous weights; the paged-side bold
    // detection (weight >= 700, convert/page.rs) must see the instantiated
    // instance's weight, not the file default. Fonts vendored in tests/fonts.
    let fonts = std::path::PathBuf::from("../../tests/fonts");
    let doc_xml = crate::common::fixture_part_with_font_dirs(
        "variable_font_weight",
        "word/document.xml",
        &[fonts],
    );

    let heavy = paragraph_containing(&doc_xml, "Heavy weight paragraph.");
    assert!(
        heavy.contains("<w:b/>"),
        "weight-700 VF text must be bold:\n{heavy}"
    );
    let light = paragraph_containing(&doc_xml, "Light weight paragraph.");
    assert!(
        !light.contains("<w:b/>"),
        "weight-300 VF text must NOT be bold:\n{light}"
    );
    let regular = paragraph_containing(&doc_xml, "Regular weight paragraph.");
    assert!(
        !regular.contains("<w:b/>"),
        "default-weight VF text must NOT be bold:\n{regular}"
    );
}

#[test]
fn deliberate_digit_run_font_override_is_kept() {
    // A digits-only run the author explicitly set in a non-body font must keep
    // its w:rFonts — only a true OpenType MATH-table fallback is normalized away.
    // See tests/fixtures/edge_digit_run_font.typ.
    let xml = fixture_doc_xml("edge_digit_run_font");
    // The run "12345" must survive carrying its declared monospace face.
    let para = paragraph_containing(&xml, "12345");
    assert!(
        para.contains("DejaVu Sans Mono"),
        "deliberate per-run font on a digit run must be preserved:\n{para}"
    );
}
