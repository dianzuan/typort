#[cfg(test)]
mod tests {
    use crate::world::TyportWorld;
    use std::path::Path;

    #[test]
    fn can_access_realized_content_elements() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        assert!(
            result.output.is_ok(),
            "HTML compilation failed: {:?}",
            result.output.err()
        );

        let html_doc = result.output.unwrap();
        let root = &html_doc.root;
        println!("Root tag: {}", root.tag);
        println!("Root children: {}", root.children.len());

        // Print ALL root children to understand the tag format
        print_tree(&root.children, 0);

        // Find body at depth 1
        let body = find_element_by_tag(&root.children, "body").expect("no body found in tree");
        println!("\n=== BODY ({} children) ===", body.children.len());
        print_tree(&body.children, 0);
    }

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
                // Recurse one level
                if let Some(found) = find_element_by_tag(&e.children, tag_name) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn print_tree(children: &[typst_html::HtmlNode], depth: usize) {
        let indent = "  ".repeat(depth);
        for (i, child) in children.iter().enumerate() {
            match child {
                typst_html::HtmlNode::Element(elem) => {
                    println!(
                        "{indent}[{i}] <{}> ({} children)",
                        elem.tag,
                        elem.children.len()
                    );
                    if depth < 2 {
                        print_tree(&elem.children, depth + 1);
                    }
                }
                typst_html::HtmlNode::Text(text, _) => {
                    let preview: String = text.chars().take(60).collect();
                    let suffix = if text.len() > 60 { "..." } else { "" };
                    println!("{indent}[{i}] TEXT: \"{preview}{suffix}\"");
                }
                typst_html::HtmlNode::Frame(_) => {
                    println!("{indent}[{i}] FRAME (laid-out content)");
                }
                typst_html::HtmlNode::Tag(tag) => {
                    println!("{indent}[{i}] TAG: {tag:?}");
                }
            }
        }
    }

    #[test]
    fn center_dom_structure() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/center_test.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        let html_doc = result.output.unwrap();
        let root = &html_doc.root;
        let body = find_element_by_tag(&root.children, "body").unwrap_or(root);
        println!("\n=== CENTER TEST ({} children) ===", body.children.len());
        for (i, child) in body.children.iter().enumerate() {
            match child {
                typst_html::HtmlNode::Element(e) => {
                    println!("[{i}] ELEM <{}> children={}", e.tag, e.children.len());
                    print_deep(&e.children, 1);
                }
                typst_html::HtmlNode::Text(t, _) => {
                    println!("[{i}] TEXT: {:?}", &t[..t.len().min(30)]);
                }
                typst_html::HtmlNode::Frame(f) => {
                    println!("[{i}] FRAME (items={})", f.inner.items().count());
                }
                typst_html::HtmlNode::Tag(t) => {
                    println!("[{i}] TAG: {t:?}");
                }
            }
        }
    }

    #[test]
    fn complex_paper_dom_structure() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        assert!(
            result.output.is_ok(),
            "compilation failed: {:?}",
            result.output.err()
        );

        let html_doc = result.output.unwrap();
        let root = &html_doc.root;

        // Find body - search recursively
        let body = find_element_by_tag(&root.children, "body").unwrap_or(root);

        println!(
            "\n=== COMPLEX PAPER (tag={}, {} top-level children) ===",
            body.tag,
            body.children.len()
        );
        for (i, child) in body.children.iter().take(40).enumerate() {
            match child {
                typst_html::HtmlNode::Element(elem) => {
                    let first_text = find_first_text(&elem.children);
                    let preview = first_text
                        .map(|t| {
                            let s: String = t.chars().take(40).collect();
                            format!(" → \"{s}\"")
                        })
                        .unwrap_or_default();
                    println!(
                        "[{i:2}] <{}> ({} children){preview}",
                        elem.tag,
                        elem.children.len()
                    );
                }
                typst_html::HtmlNode::Text(text, _) => {
                    let preview: String = text.chars().take(50).collect();
                    println!("[{i:2}] TEXT: \"{preview}\"");
                }
                typst_html::HtmlNode::Frame(_) => {
                    println!("[{i:2}] FRAME");
                }
                typst_html::HtmlNode::Tag(_) => {
                    println!("[{i:2}] TAG");
                }
            }
        }
    }

    fn find_first_text(children: &[typst_html::HtmlNode]) -> Option<&str> {
        for child in children {
            match child {
                typst_html::HtmlNode::Text(t, _) => return Some(t.as_str()),
                typst_html::HtmlNode::Element(e) => {
                    if let Some(t) = find_first_text(&e.children) {
                        return Some(t);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn print_deep(children: &[typst_html::HtmlNode], depth: usize) {
        let indent = "  ".repeat(depth);
        for (i, child) in children.iter().enumerate() {
            match child {
                typst_html::HtmlNode::Element(elem) => {
                    let attrs: Vec<String> = elem
                        .attrs
                        .0
                        .iter()
                        .map(|(k, v)| format!("{k}=\"{v}\""))
                        .collect();
                    let attr_str = if attrs.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", attrs.join(" "))
                    };
                    println!(
                        "{indent}[{i}] <{}{}> ({} children)",
                        elem.tag,
                        attr_str,
                        elem.children.len()
                    );
                    print_deep(&elem.children, depth + 1);
                }
                typst_html::HtmlNode::Text(text, _) => {
                    let preview: String = text.chars().take(80).collect();
                    println!("{indent}[{i}] TEXT: \"{preview}\"");
                }
                typst_html::HtmlNode::Frame(_) => {
                    println!("{indent}[{i}] FRAME");
                }
                typst_html::HtmlNode::Tag(tag) => {
                    println!("{indent}[{i}] TAG: {tag:?}");
                }
            }
        }
    }

    #[test]
    fn complex_paper_footnote_structure() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        let html_doc = result.output.unwrap();
        let root = &html_doc.root;
        let body = find_element_by_tag(&root.children, "body").unwrap_or(root);

        // Look for elements with role=doc-footnote or similar footnote-content structures
        println!("\n=== ALL ELEMENTS (looking for footnote content) ===");
        for (i, child) in body.children.iter().enumerate() {
            match child {
                typst_html::HtmlNode::Element(elem) => {
                    let tag_str = format!("{}", elem.tag);
                    // Print all elements with their attrs
                    let attrs: Vec<String> = elem
                        .attrs
                        .0
                        .iter()
                        .map(|(k, v)| format!("{k}=\"{v}\""))
                        .collect();
                    if !attrs.is_empty()
                        || tag_str.contains("aside")
                        || tag_str.contains("section")
                        || tag_str.contains("footer")
                    {
                        println!(
                            "[{i}] <{tag_str} {}> ({} children)",
                            attrs.join(" "),
                            elem.children.len()
                        );
                        if elem.children.len() <= 10 {
                            print_deep(&elem.children, 1);
                        }
                    }
                }
                typst_html::HtmlNode::Tag(tag) => {
                    let tag_str = format!("{tag:?}");
                    if tag_str.contains("footnote") || tag_str.contains("note") {
                        println!("[{i}] TAG: {tag_str}");
                    }
                }
                _ => {}
            }
        }

        // Also look at the very end of body children - footnotes might be there
        println!("\n=== LAST 40 children of body ===");
        let start = body.children.len().saturating_sub(40);
        for (i, child) in body.children.iter().enumerate().skip(start) {
            match child {
                typst_html::HtmlNode::Element(elem) => {
                    let attrs: Vec<String> = elem
                        .attrs
                        .0
                        .iter()
                        .map(|(k, v)| format!("{k}=\"{v}\""))
                        .collect();
                    println!(
                        "[{i}] <{}{}> ({} children)",
                        elem.tag,
                        if attrs.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", attrs.join(" "))
                        },
                        elem.children.len()
                    );
                    if elem.children.len() <= 6 {
                        print_deep(&elem.children, 1);
                    }
                }
                typst_html::HtmlNode::Text(text, _) => {
                    let preview: String = text.chars().take(60).collect();
                    println!("[{i}] TEXT: \"{preview}\"");
                }
                typst_html::HtmlNode::Tag(tag) => {
                    println!("[{i}] TAG: {tag:?}");
                }
                typst_html::HtmlNode::Frame(_) => {}
            }
        }
    }

    #[test]
    fn math_via_introspector() {
        use typst::foundations::NativeElement;
        use typst_library::math::EquationElem;

        let world = TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        let html_doc = result.output.unwrap();

        let equations = html_doc.introspector.query(&EquationElem::ELEM.select());
        println!("\n=== EQUATIONS FOUND: {} ===", equations.len());
        assert!(equations.len() >= 2, "should find at least 2 equations");

        for (i, eq_content) in equations.iter().enumerate() {
            let eq = eq_content.to_packed::<EquationElem>().unwrap();
            println!("[{i}] block={:?}", eq.block);
            println!("    body func: {}", eq.body.func().name());
            // Print the math tree using Debug
            println!("    body debug: {:?}", eq.body);
        }
    }

    #[test]
    fn math_test_dom_structure() {
        let world = TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
        let result = typst::compile::<typst_html::HtmlDocument>(&world);
        let html_doc = result.output.unwrap();
        let root = &html_doc.root;
        let body = find_element_by_tag(&root.children, "body").unwrap_or(root);

        println!("\n=== MATH TEST DOM ===");
        print_deep(&body.children, 0);
    }
}
