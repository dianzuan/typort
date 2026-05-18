#[cfg(test)]
mod tests {
    use crate::world::TyportWorld;
    use std::path::Path;

    fn find_element_by_tag<'a>(
        children: &'a [typst_html::HtmlNode],
        tag_name: &str,
    ) -> Option<&'a typst_html::HtmlElement> {
        for child in children {
            if let typst_html::HtmlNode::Element(e) = child {
                let tag_str = format!("{}", e.tag);
                if tag_str == tag_name
                    || tag_str == format!("<<{tag_name}>>")
                    || tag_str.contains(tag_name)
                {
                    return Some(e);
                }
                if let Some(found) = find_element_by_tag(&e.children, tag_name) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn count_elements_by_tag(children: &[typst_html::HtmlNode], tag_name: &str) -> usize {
        let mut count = 0;
        for child in children {
            if let typst_html::HtmlNode::Element(e) = child {
                let tag_str = format!("{}", e.tag);
                if tag_str == tag_name || tag_str.contains(tag_name) {
                    count += 1;
                }
                count += count_elements_by_tag(&e.children, tag_name);
            }
        }
        count
    }

    fn collect_all_text(children: &[typst_html::HtmlNode]) -> String {
        let mut text = String::new();
        for child in children {
            match child {
                typst_html::HtmlNode::Text(t, _) => text.push_str(t),
                typst_html::HtmlNode::Element(e) => text.push_str(&collect_all_text(&e.children)),
                _ => {}
            }
        }
        text
    }

    fn has_attr_value(elem: &typst_html::HtmlElement, attr_name: &str, attr_value: &str) -> bool {
        for (k, v) in &elem.attrs.0 {
            if format!("{k}") == attr_name && format!("{v}") == attr_value {
                return true;
            }
        }
        false
    }

    // --- HTML compilation produces expected DOM structure ---

    #[test]
    fn hello_html_has_body_with_children() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        let html_doc = result.output.unwrap();
        let body = find_element_by_tag(&html_doc.root.children, "body")
            .expect("HTML output must have a body element");
        assert!(
            !body.children.is_empty(),
            "body should have at least one child"
        );
    }

    #[test]
    fn hello_html_has_heading() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();
        let body = find_element_by_tag(&html_doc.root.children, "body").unwrap();
        let h2 = find_element_by_tag(&body.children, "h2");
        assert!(h2.is_some(), "hello.typ should produce an h2 heading");
        let heading_text = collect_all_text(&h2.unwrap().children);
        assert!(
            heading_text.contains("Hello"),
            "heading text should contain 'Hello', got: {heading_text}"
        );
    }

    #[test]
    fn hello_html_has_paragraphs() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();
        let body = find_element_by_tag(&html_doc.root.children, "body").unwrap();
        let p_count = count_elements_by_tag(&body.children, "p");
        assert!(
            p_count >= 1,
            "hello.typ should have at least 1 paragraph, got {p_count}"
        );
    }

    // --- Complex paper DOM structure ---

    #[test]
    fn complex_paper_html_has_multiple_heading_levels() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();
        let body =
            find_element_by_tag(&html_doc.root.children, "body").unwrap_or(&html_doc.root);
        let h2_count = count_elements_by_tag(&body.children, "h2");
        let h3_count = count_elements_by_tag(&body.children, "h3");
        assert!(
            h2_count >= 3,
            "complex paper should have at least 3 h2 headings, got {h2_count}"
        );
        assert!(
            h3_count >= 2,
            "complex paper should have at least 2 h3 headings, got {h3_count}"
        );
    }

    #[test]
    fn complex_paper_html_has_table() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();
        let body =
            find_element_by_tag(&html_doc.root.children, "body").unwrap_or(&html_doc.root);
        let table = find_element_by_tag(&body.children, "table");
        assert!(table.is_some(), "complex paper should have a table element");
    }

    #[test]
    fn complex_paper_html_has_footnote_endnotes_section() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();
        let body =
            find_element_by_tag(&html_doc.root.children, "body").unwrap_or(&html_doc.root);

        let has_endnotes = body.children.iter().any(|child| {
            if let typst_html::HtmlNode::Element(elem) = child {
                has_attr_value(elem, "role", "doc-endnotes")
            } else {
                false
            }
        });
        assert!(
            has_endnotes,
            "complex paper should have a section with role=doc-endnotes"
        );
    }

    // --- Math introspector ---

    #[test]
    fn math_introspector_finds_equations() {
        use typst::foundations::NativeElement;
        use typst_library::math::EquationElem;

        let world = TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();

        let equations = html_doc.introspector.query(&EquationElem::ELEM.select());
        assert!(
            equations.len() >= 2,
            "math_test.typ should have at least 2 equations, found {}",
            equations.len()
        );
    }

    #[test]
    fn math_introspector_detects_block_and_inline() {
        use typst::foundations::NativeElement;
        use typst_library::math::EquationElem;

        let world = TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
        let html_doc = typst::compile::<typst_html::HtmlDocument>(&world)
            .output
            .unwrap();

        let equations = html_doc.introspector.query(&EquationElem::ELEM.select());
        let mut has_inline = false;
        let mut has_block = false;
        for eq_content in &equations {
            let eq = eq_content.to_packed::<EquationElem>().unwrap();
            let is_block = *eq.block.as_option().as_ref().unwrap_or(&false);
            if is_block {
                has_block = true;
            } else {
                has_inline = true;
            }
        }
        assert!(has_inline, "math_test.typ should have inline equations");
        assert!(has_block, "math_test.typ should have block equations");
    }

    // --- Center test DOM ---

    #[test]
    fn center_test_html_compiles() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/center_test.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        assert!(result.output.is_ok(), "center_test.typ should compile");
    }
}
