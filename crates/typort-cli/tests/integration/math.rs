//! Math (OMML) rendering tests.

use crate::common;
use crate::common::{fixture_doc_xml, paragraph_containing};
use std::io::Cursor;
use std::path::Path;

#[test]
fn inline_math_spacing_cjk_tight_latin_spaced() {
    // Regression: the inline-equation merge must not insert a literal space between
    // CJK text and an equation (Typst renders 标量M tight); it must keep the space
    // for Latin text (Typst trims it, Word needs it back). See
    // tests/fixtures/edge_cjk_inline_math_spacing.typ.
    //
    // The run-coalescing post-pass folds the (formerly standalone) space run into
    // the adjacent text run, so we assert the space is present *in the neighbouring
    // run's text* (tight for CJK, spaced for Latin) rather than as its own run.
    let doc_xml = fixture_doc_xml("edge_cjk_inline_math_spacing");

    let cjk = paragraph_containing(&doc_xml, "标量");
    assert!(
        cjk.contains("标量</w:t>") && !cjk.contains("标量 "),
        "CJK text adjacent to inline math must stay tight (no inserted space):\n{cjk}"
    );
    let latin = paragraph_containing(&doc_xml, "the value");
    assert!(
        latin.contains("the value </w:t>") && latin.contains(" is here."),
        "Latin text around inline math must keep its space:\n{latin}"
    );
}

#[test]
fn par_wrapped_inline_math_keeps_prose_with_math() {
    // Regression: prose inside an author par()[...] wrapper around inline math was
    // dropped (only the equations survived as an orphan math paragraph). See
    // tests/fixtures/edge_par_wraps_inline_math.typ.
    let doc_xml = fixture_doc_xml("edge_par_wraps_inline_math");

    // The prose around the inline math must survive — especially the text AFTER
    // the last equation, which the skip dropped entirely.
    assert!(
        doc_xml.contains("Lead") && doc_xml.contains("matters here"),
        "wrapped-par prose must not be dropped"
    );
    // The prose must sit in the SAME paragraph as its OMML, not be split off into
    // an orphan math-only paragraph.
    let lead_para = paragraph_containing(&doc_xml, "Lead");
    assert!(
        lead_para.contains("<m:oMath>"),
        "wrapped-par prose and its inline math must share one paragraph"
    );
    assert!(
        lead_para.contains("matters here"),
        "prose after the last inline equation must stay in the same paragraph"
    );
    // No recovery-injected duplicate of the prose.
    assert_eq!(
        doc_xml.matches("matters here").count(),
        1,
        "wrapped-par prose must appear exactly once"
    );
    // No-regression: a flat body paragraph with inline math also interleaves prose
    // and OMML in one paragraph (the existing merge path must be unaffected).
    let body_para = paragraph_containing(&doc_xml, "Flat body paragraph");
    assert!(
        body_para.contains("<m:oMath>"),
        "flat body paragraph must keep prose and inline math together"
    );
}

#[test]
fn math_test_produces_omml() {
    let doc_xml = fixture_doc_xml("math_test");

    // Verify OMML namespace is present
    assert!(
        doc_xml.contains("xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\""),
        "document.xml should have the math namespace"
    );

    // Verify inline equation produces m:oMath
    assert!(
        doc_xml.contains("<m:oMath>"),
        "document.xml should contain <m:oMath> for equations"
    );

    // Verify block equation produces m:oMathPara
    assert!(
        doc_xml.contains("<m:oMathPara>"),
        "document.xml should contain <m:oMathPara> for block equations"
    );

    // Verify superscript structure (x^2)
    assert!(
        doc_xml.contains("<m:sSup>"),
        "document.xml should contain <m:sSup> for superscripts"
    );

    // Verify fraction structure (frac(n(n+1), 2))
    assert!(
        doc_xml.contains("<m:f>"),
        "document.xml should contain <m:f> for fractions"
    );
    assert!(
        doc_xml.contains("<m:num>"),
        "document.xml should contain <m:num> for fraction numerator"
    );
    assert!(
        doc_xml.contains("<m:den>"),
        "document.xml should contain <m:den> for fraction denominator"
    );

    // Verify nary (summation) structure
    assert!(
        doc_xml.contains("<m:nary>"),
        "document.xml should contain <m:nary> for summation"
    );

    // Verify delimiter structure (parentheses in n(n+1))
    assert!(
        doc_xml.contains("<m:d>"),
        "document.xml should contain <m:d> for delimiters"
    );

    // Verify math runs contain expected symbols
    assert!(
        doc_xml.contains("<m:t>x</m:t>"),
        "document.xml should contain math text 'x'"
    );
    assert!(
        doc_xml.contains("<m:t>2</m:t>"),
        "document.xml should contain math text '2'"
    );
}

#[test]
fn math_styled_wrappers_and_dif_are_not_dropped() {
    // Regression: bold()/bb()/cal() and the upright differential `dif` used to
    // fall into convert_content's silent "unknown element" skip, producing empty
    // <m:e> bases and vanishing glyphs. See tests/fixtures/math_styled_and_dif.typ.
    let doc_xml = fixture_doc_xml("math_styled_and_dif");
    let packed: String = doc_xml.chars().filter(|c| !c.is_whitespace()).collect();

    // bb(R) -> blackboard-bold ℝ (U+211D), used twice in the fixture.
    assert!(
        doc_xml.contains('\u{211D}'),
        "bb(R) should render as blackboard-bold ℝ, not be dropped"
    );
    // bold(e) -> mathematical bold-italic 𝒆 (U+1D486).
    assert!(
        doc_xml.contains('\u{1D486}'),
        "bold(e) should render as bold 𝒆, not be dropped"
    );
    // bold(s) -> 𝒔 (U+1D494, mathematical bold-italic small s).
    assert!(
        doc_xml.contains('\u{1D494}'),
        "bold(s) should render as bold 𝒔, not be dropped"
    );
    // No styled atom may leave an empty math base behind.
    assert!(
        !packed.contains("<m:e></m:e>"),
        "styled math atoms must not leave empty <m:e> bases"
    );
    // dif = upright(d): forced non-italic via an explicit m:sty value "p".
    assert!(
        doc_xml.contains("<m:sty m:val=\"p\"/>"),
        "upright(d) (dif) should force an upright run via m:sty p"
    );
}

#[test]
fn numbered_equation_has_right_aligned_number() {
    let doc_xml = fixture_doc_xml("numbered_eq");

    // Verify equation number "(1)" appears in the document
    assert!(
        doc_xml.contains("(1)"),
        "document.xml should contain equation number (1)"
    );
    // Verify equation number "(2)" appears for the second equation
    assert!(
        doc_xml.contains("(2)"),
        "document.xml should contain equation number (2)"
    );
    // Word-native numbered equation: a center tab centers the math and a right tab
    // holds the number, around inline <m:oMath> (not a standalone block oMathPara,
    // which cannot share a line with the trailing number).
    assert!(
        doc_xml.contains(r#"<w:tab w:val="center""#),
        "numbered equation should be centered with a center tab stop"
    );
    assert!(
        doc_xml.contains(r#"<w:tab w:val="right""#),
        "numbered equation should hold the number at a right tab stop"
    );
    assert!(
        doc_xml.contains("<m:oMath>"),
        "document.xml should contain the equation as inline OMML"
    );
    // All equations here are numbered, so none should be a standalone block.
    assert!(
        !doc_xml.contains("<m:oMathPara>"),
        "a numbered equation must not be a standalone block oMathPara"
    );
}

#[test]
fn numbered_equation_document_model_has_numbers() {
    use typort_ooxml::document::{BlockElement, InlineElement};

    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/numbered_eq.typ")).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    // Find paragraphs with numbered equations
    let numbered_eqs: Vec<&str> = doc
        .body
        .elements
        .iter()
        .filter_map(|e| {
            if let BlockElement::Paragraph(p) = e {
                for inline in &p.inlines {
                    if let InlineElement::Math {
                        equation_number: Some(num),
                        ..
                    } = inline
                    {
                        return Some(num.as_str());
                    }
                }
            }
            None
        })
        .collect();

    assert_eq!(
        numbered_eqs.len(),
        2,
        "should have 2 numbered equations, got {numbered_eqs:?}"
    );
    assert_eq!(numbered_eqs[0], "(1)");
    assert_eq!(numbered_eqs[1], "(2)");
}

#[test]
fn math_fraction_produces_m_f() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:f>"),
        "document.xml should contain <m:f> for fraction"
    );
    assert!(
        doc_xml.contains("<m:num>"),
        "document.xml should contain <m:num> for fraction numerator"
    );
    assert!(
        doc_xml.contains("<m:den>"),
        "document.xml should contain <m:den> for fraction denominator"
    );
}

#[test]
fn math_square_root_produces_m_rad() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:rad>"),
        "document.xml should contain <m:rad> for square root"
    );
    assert!(
        doc_xml.contains("<m:degHide m:val=\"1\"/>"),
        "square root should hide degree with <m:degHide m:val=\"1\"/>"
    );
}

#[test]
fn math_cube_root_has_degree() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // There should be a <m:rad> that contains <m:deg> with content (the index "3")
    assert!(
        doc_xml.contains("<m:deg>"),
        "cube root should have <m:deg> element for the index"
    );
    // The cube root's degree should contain the text "3"
    assert!(
        doc_xml.contains("<m:t>3</m:t>"),
        "cube root degree should contain the text '3'"
    );
}

#[test]
fn math_subscript_produces_m_ssub() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:sSub>"),
        "document.xml should contain <m:sSub> for subscript"
    );
}

#[test]
fn math_superscript_produces_m_ssup() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:sSup>"),
        "document.xml should contain <m:sSup> for superscript"
    );
}

#[test]
fn math_sub_and_sup_produces_m_ssubsup() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:sSubSup>"),
        "document.xml should contain <m:sSubSup> for combined sub+superscript"
    );
}

#[test]
fn math_summation_produces_m_nary() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:nary>"),
        "document.xml should contain <m:nary> for summation"
    );
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{2211}\"/>"),
        "summation should have <m:chr m:val=\"\\u{{2211}}\"/> (summation symbol)"
    );
}

#[test]
fn math_product_produces_m_nary() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{220F}\"/>"),
        "product should have <m:chr m:val=\"\\u{{220F}}\"/> (product symbol)"
    );
}

#[test]
fn math_nary_operand_is_inside_m_e() {
    // The summand must live inside the n-ary's <m:e>, not be left empty with the
    // operand detached as siblings (ECMA-376 violation). For `sum_(i=1)^n i^2`,
    // the m:e following m:sup must contain the summand `i^2` (an m:sSup).
    let xml = fixture_doc_xml("issue_nary_operand_body");
    let start = xml.find("<m:nary>").expect("expected an n-ary for the sum");
    let end = xml.find("</m:nary>").expect("n-ary should close");
    let nary = &xml[start..end];
    assert!(
        !nary.contains("</m:sup><m:e/>") && !nary.contains("</m:sup><m:e></m:e>"),
        "n-ary body <m:e> must not be empty: {nary}"
    );
    // The summand i^2 (an m:sSup) lives inside the n-ary body.
    assert!(
        nary.contains("<m:sSup>"),
        "summand i^2 should be inside the n-ary body, got: {nary}"
    );
}

#[test]
fn math_nary_operand_stops_at_relation() {
    // Operand boundary = a Relation-class symbol. In `sum_(i=1)^n i^2 = S`, the
    // `= S` must fall OUTSIDE the n-ary's operand body, not get pulled into the
    // summand. (Note the `=` inside the lower limit `i=1` is in <m:sub> and is
    // legitimately within the n-ary — so we check the operand <m:e> specifically,
    // not the whole n-ary.) The fixture has exactly one n-ary.
    let xml = fixture_doc_xml("issue_nary_operand_body");
    let nary_end = xml.find("</m:nary>").expect("expected an n-ary");
    let nary = &xml[..nary_end];
    // The operand body is the <m:e> that follows </m:sup>.
    let body_start = nary.find("</m:sup>").expect("n-ary has an upper limit") + "</m:sup>".len();
    let operand = &nary[body_start..];
    assert!(
        !operand.contains("<m:t>=</m:t>"),
        "the `=` relation must NOT be inside the n-ary operand body: {operand}"
    );
    let after = &xml[nary_end..];
    assert!(
        after.contains("<m:t>=</m:t>") && after.contains("<m:t>S</m:t>"),
        "the `= S` must appear after </m:nary>"
    );
}

#[test]
fn math_nested_fraction() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // Count occurrences of <m:f> — should be at least 3: the simple frac, and 2 from nested
    let count = doc_xml.matches("<m:f>").count();
    assert!(
        count >= 3,
        "should have at least 3 <m:f> elements (1 simple + 2 nested), got {count}"
    );
}

#[test]
fn math_greek_letters() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:t>\u{03B1}</m:t>"),
        "should contain Greek alpha (\u{03B1})"
    );
    assert!(
        doc_xml.contains("<m:t>\u{03B2}</m:t>"),
        "should contain Greek beta (\u{03B2})"
    );
    assert!(
        doc_xml.contains("<m:t>\u{03B3}</m:t>"),
        "should contain Greek gamma (\u{03B3})"
    );
}

#[test]
fn math_differential_dif_emitted() {
    // Regression (typst 0.15 migration): 0.15 wraps `dif` in a ClassElem
    // (Unary, upright d) that 0.14 emitted bare. convert_content gained a ClassElem
    // arm that descends the body; without it the differential `d` was silently
    // dropped, turning `integral y dif x` into `integral y x` (a math corruption).
    let doc_xml = fixture_doc_xml("issue_math_differential");
    // Concatenate the OMML text runs (typort emits attribute-less <m:t>).
    let omml: String = doc_xml
        .match_indices("<m:t>")
        .filter_map(|(i, _)| {
            let rest = &doc_xml[i + 5..];
            rest.find("</m:t>").map(|end| &rest[..end])
        })
        .collect();
    assert!(
        omml.contains("ydx"),
        "the differential `d` (from `dif x`) must survive in the OMML \
         (`integral y dif x` => `y d x`), got m:t = {omml:?}"
    );
}

#[test]
fn math_matrix_produces_m_m() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:m>"),
        "document.xml should contain <m:m> for matrix"
    );
    assert!(
        doc_xml.contains("<m:mr>"),
        "document.xml should contain <m:mr> for matrix row"
    );
}

#[test]
fn math_accent_hat_produces_m_acc() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:acc>"),
        "document.xml should contain <m:acc> for accent"
    );
    assert!(
        doc_xml.contains("<m:accPr>"),
        "document.xml should contain <m:accPr> for accent properties"
    );
    // hat accent should use combining circumflex U+0302
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{0302}\"/>"),
        "hat accent should have chr U+0302 (combining circumflex)"
    );
}

#[test]
fn math_accent_arrow_produces_m_acc_with_arrow_chr() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // arrow accent should use combining right arrow above U+20D7
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{20D7}\"/>"),
        "arrow accent should have chr U+20D7 (combining right arrow above)"
    );
}

#[test]
fn math_overline_produces_m_bar_top() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:bar>"),
        "document.xml should contain <m:bar> for overline"
    );
    assert!(
        doc_xml.contains("<m:pos m:val=\"top\"/>"),
        "overline should have pos=top"
    );
}

#[test]
fn math_underline_produces_m_bar_bot() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:pos m:val=\"bot\"/>"),
        "underline should have pos=bot"
    );
}

#[test]
fn math_named_func_produces_m_func() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:func>"),
        "document.xml should contain <m:func> for named function"
    );
    assert!(
        doc_xml.contains("<m:fName>"),
        "document.xml should contain <m:fName> for function name"
    );
    // sin should appear as plain-style text
    assert!(
        doc_xml.contains("<m:t>sin</m:t>"),
        "function name should contain 'sin'"
    );
}

#[test]
fn math_cases_produces_m_d_with_m_eqarr() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:eqArr>"),
        "document.xml should contain <m:eqArr> for cases"
    );
    // Cases should have a left brace delimiter
    assert!(
        doc_xml.contains("<m:begChr m:val=\"{\"/>"),
        "cases should have opening brace delimiter"
    );
    // Cases should suppress the closing delimiter
    assert!(
        doc_xml.contains("<m:endChr m:val=\"\"/>"),
        "cases should have empty closing delimiter"
    );
}

#[test]
fn math_underbrace_produces_m_groupchr() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    assert!(
        doc_xml.contains("<m:groupChr>"),
        "document.xml should contain <m:groupChr> for underbrace"
    );
    assert!(
        doc_xml.contains("<m:groupChrPr>"),
        "document.xml should contain <m:groupChrPr> for group char properties"
    );
    // Underbrace uses U+23DF
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{23DF}\"/>"),
        "underbrace should have chr U+23DF (bottom curly bracket)"
    );
}

#[test]
fn math_overbrace_produces_m_groupchr() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // Overbrace uses U+23DE
    assert!(
        doc_xml.contains("<m:chr m:val=\"\u{23DE}\"/>"),
        "overbrace should have chr U+23DE (top curly bracket)"
    );
}

#[test]
fn math_underbrace_annotation_produces_m_limlow() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // Underbrace with annotation should be wrapped in m:limLow
    assert!(
        doc_xml.contains("<m:limLow>"),
        "underbrace with annotation should produce <m:limLow>"
    );
    assert!(
        doc_xml.contains("<m:lim>"),
        "should have <m:lim> element for the annotation"
    );
}

#[test]
fn math_overbrace_annotation_produces_m_limupp() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // Overbrace with annotation should be wrapped in m:limUpp
    assert!(
        doc_xml.contains("<m:limUpp>"),
        "overbrace with annotation should produce <m:limUpp>"
    );
}

#[test]
fn math_vector_produces_m_m_in_delimiters() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // vec() produces a column vector with parentheses and a matrix inside
    // It should have at least 3 m:mr rows (for vec(1, 2, 3))
    let mr_count = doc_xml.matches("<m:mr>").count();
    assert!(
        mr_count >= 3,
        "vector should produce at least 3 <m:mr> rows, got {mr_count}"
    );
}

#[test]
fn math_aligned_equation_produces_standalone_eqarr() {
    let doc_xml = common::fixture_doc_xml("math_unit");
    // The math_unit.typ now has an aligned equation: x &= 1 + 2 \ &= 3
    // This should produce m:eqArr directly inside m:oMath (not wrapped in m:d like cases)
    // Count eqArr occurrences — should be at least 2 (1 from cases + 1 from aligned eq)
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert!(
        eqarr_count >= 2,
        "should have at least 2 <m:eqArr> (cases + aligned equation), got {eqarr_count}"
    );
}

#[test]
fn aligned_equation_produces_m_eqarr() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // Multi-line aligned equations should produce m:eqArr
    assert!(
        doc_xml.contains("<m:eqArr>"),
        "document.xml should contain <m:eqArr> for aligned equations"
    );
}

#[test]
fn aligned_equation_has_correct_row_count() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // The fixture has:
    //   - Simple alignment: 2 lines (2 m:e)
    //   - Multi-line with expressions: 2 lines (2 m:e)
    //   - Three lines: 3 lines (3 m:e)
    // Total m:e inside eqArr = 7
    // But m:e is also used for other purposes (e.g., delimiters, superscripts),
    // so we count eqArr instances instead.
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert_eq!(
        eqarr_count, 3,
        "should have 3 eqArr elements (one per aligned equation), got {eqarr_count}"
    );
}

#[test]
fn aligned_equation_simple_has_two_rows() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // Find the first m:eqArr and count its direct m:e children
    // The simple alignment "x &= 1 + 2 \ &= 3" should have 2 rows
    if let Some(start) = doc_xml.find("<m:eqArr>") {
        if let Some(end) = doc_xml[start..].find("</m:eqArr>") {
            let eqarr_xml = &doc_xml[start..start + end + "</m:eqArr>".len()];
            let row_count = eqarr_xml.matches("<m:e>").count();
            assert_eq!(
                row_count, 2,
                "simple aligned equation should have 2 rows, got {row_count} in:\n{eqarr_xml}"
            );
        } else {
            panic!("could not find closing </m:eqArr>");
        }
    } else {
        panic!("could not find <m:eqArr> in document.xml");
    }
}

#[test]
fn aligned_equation_three_lines_has_three_rows() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // Find the third m:eqArr (3-line equation: a = b+c, = d+e, = f)
    let mut search_from = 0;
    for _ in 0..2 {
        if let Some(pos) = doc_xml[search_from..].find("<m:eqArr>") {
            search_from += pos + "<m:eqArr>".len();
        } else {
            panic!("could not find enough <m:eqArr> elements");
        }
    }
    // Now find the third one
    if let Some(start_offset) = doc_xml[search_from..].find("<m:eqArr>") {
        let start = search_from + start_offset;
        if let Some(end_offset) = doc_xml[start..].find("</m:eqArr>") {
            let eqarr_xml = &doc_xml[start..start + end_offset + "</m:eqArr>".len()];
            let row_count = eqarr_xml.matches("<m:e>").count();
            assert_eq!(
                row_count, 3,
                "three-line aligned equation should have 3 rows, got {row_count}"
            );
        } else {
            panic!("could not find closing </m:eqArr>");
        }
    } else {
        panic!("could not find third <m:eqArr>");
    }
}

#[test]
fn aligned_equation_contains_alignment_ampersand() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // The alignment point should be emitted as &amp; (XML-escaped ampersand)
    // inside math runs within eqArr
    assert!(
        doc_xml.contains("&amp;"),
        "aligned equations should contain &amp; for alignment points"
    );
}

#[test]
fn aligned_equation_is_wrapped_in_omathpara() {
    let doc_xml = common::fixture_doc_xml("aligned_equations");
    // Block aligned equations should be inside m:oMathPara
    assert!(
        doc_xml.contains("<m:oMathPara>"),
        "block aligned equations should be wrapped in m:oMathPara"
    );
    // Each eqArr should be inside oMathPara > oMath
    // Find a pattern that confirms oMathPara > oMath > eqArr nesting
    let omathpara_count = doc_xml.matches("<m:oMathPara>").count();
    let eqarr_count = doc_xml.matches("<m:eqArr>").count();
    assert_eq!(
        omathpara_count, eqarr_count,
        "each eqArr should have a corresponding oMathPara wrapper"
    );
}

#[test]
fn math_in_heading_produces_omml() {
    let doc_xml = fixture_doc_xml("math_in_heading");

    assert!(
        doc_xml.contains("m:oMath"),
        "heading with inline math should produce m:oMath element"
    );
    assert!(doc_xml.contains("Heading2"), "should still be a heading");
}

#[test]
fn inline_math_produces_single_paragraph() {
    let doc_xml = common::fixture_doc_xml("inline_math_in_text");

    // Count <w:p> elements — should be exactly 1 (the single sentence)
    let p_count = doc_xml.matches("<w:p>").count() + doc_xml.matches("<w:p ").count();
    assert_eq!(
        p_count, 1,
        "sentence with inline math should produce exactly 1 paragraph, got {p_count}: {doc_xml}"
    );
}

#[test]
fn inline_math_has_omath_not_omathpara() {
    let doc_xml = common::fixture_doc_xml("inline_math_in_text");

    // Inline math should use <m:oMath>, NOT <m:oMathPara>
    assert!(
        doc_xml.contains("<m:oMath>"),
        "inline math should produce <m:oMath> elements"
    );
    assert!(
        !doc_xml.contains("<m:oMathPara>"),
        "inline math should NOT produce <m:oMathPara> (that's for block equations)"
    );
}

#[test]
fn inline_math_text_runs_are_preserved() {
    let doc_xml = common::fixture_doc_xml("inline_math_in_text");

    // All text fragments should be present
    assert!(
        doc_xml.contains("Where"),
        "text run 'Where' should be present"
    );
    assert!(
        doc_xml.contains("is the dependent variable and"),
        "text run 'is the dependent variable and' should be present"
    );
    assert!(
        doc_xml.contains("is the explanatory variable."),
        "text run 'is the explanatory variable.' should be present"
    );
}

#[test]
fn inline_math_interleaved_with_text_in_same_paragraph() {
    let doc_xml = common::fixture_doc_xml("inline_math_in_text");

    // Find the single <w:p> and verify it contains both text and math
    // by checking that text runs and math elements are siblings inside one <w:p>
    let p_start = doc_xml.find("<w:p>").expect("should have a <w:p>");
    let p_end = doc_xml[p_start..]
        .find("</w:p>")
        .expect("should have </w:p>")
        + p_start;
    let p_content = &doc_xml[p_start..p_end];

    // Should contain both text runs and math
    assert!(
        p_content.contains("<w:r>") && p_content.contains("<m:oMath>"),
        "the single paragraph should contain both <w:r> text runs and <m:oMath> elements"
    );

    // Should contain at least 2 math elements (for $y$ and $x$)
    let math_count = p_content.matches("<m:oMath>").count();
    assert!(
        math_count >= 2,
        "should have at least 2 inline math elements, got {math_count}"
    );
}

#[test]
fn equation_label_produces_bookmark() {
    let doc_xml = fixture_doc_xml("equation_label");

    assert!(
        doc_xml.contains("w:bookmarkStart") && doc_xml.contains("eq:pythagoras"),
        "document.xml should contain a bookmarkStart with name eq:pythagoras"
    );
    assert!(
        doc_xml.contains("w:bookmarkEnd"),
        "document.xml should contain a bookmarkEnd for the equation bookmark"
    );
}

#[test]
fn equation_label_cross_reference_produces_ref_field() {
    let doc_xml = fixture_doc_xml("equation_label");

    assert!(
        doc_xml.contains("REF eq:pythagoras"),
        "document.xml should contain a REF field code pointing at eq:pythagoras"
    );
}

#[test]
fn edge_math_in_footnote_preserved() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/edge_math_in_footnote.typ"))
            .unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let fn_xml = std::io::read_to_string(reader.by_name("word/footnotes.xml").unwrap()).unwrap();

    assert!(
        fn_xml.contains("m:oMath"),
        "footnotes.xml should contain m:oMath elements for math in footnotes"
    );
}

#[test]
fn edge_super_sub_in_heading_preserved() {
    let doc_xml = fixture_doc_xml("edge_super_sub_in_heading");

    assert!(
        doc_xml.contains("w:vertAlign w:val=\"subscript\""),
        "heading with H₂O should have subscript vertAlign"
    );
    assert!(
        doc_xml.contains("w:vertAlign w:val=\"superscript\""),
        "heading with x² should have superscript vertAlign"
    );
}
