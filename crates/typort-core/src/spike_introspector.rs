#[cfg(test)]
mod tests {
    use crate::world::TyportWorld;
    use std::path::Path;
    use typst::foundations::NativeElement;
    use typst::introspection::Tag;
    use typst_html::{HtmlDocument, HtmlNode};

    const FIXTURE: &str = "../../tests/fixtures/spike_introspector.typ";

    fn compile_html() -> HtmlDocument {
        let world = TyportWorld::new(Path::new(FIXTURE)).unwrap();
        typst::compile::<HtmlDocument>(&world).output.unwrap()
    }

    // =====================================================
    // 1. HeadingElem — 能否查到标题、拿到 level 和 body？
    // =====================================================
    #[test]
    fn introspector_finds_headings_with_level() {
        use typst_library::model::HeadingElem;

        let doc = compile_html();
        let headings = doc.introspector.query(&HeadingElem::ELEM.select());

        println!("\n=== HEADINGS ({}) ===", headings.len());
        for (i, h) in headings.iter().enumerate() {
            let heading = h.to_packed::<HeadingElem>().unwrap();
            let level = heading.resolve_level(typst::foundations::StyleChain::default());
            println!("[{i}] level={level} body={:?}", heading.body);
        }

        assert!(headings.len() >= 2, "should find at least 2 headings");
    }

    // =====================================================
    // 2. ImageElem — 能否查到图片？
    // =====================================================
    #[test]
    fn introspector_finds_images() {
        use typst_library::visualize::ImageElem;

        let doc = compile_html();
        let images = doc.introspector.query(&ImageElem::ELEM.select());

        println!("\n=== IMAGES ({}) ===", images.len());
        for (i, img_content) in images.iter().enumerate() {
            let img_elem = img_content.to_packed::<ImageElem>().unwrap();
            println!("[{i}] source={:?}", img_elem.source);
        }

        assert!(!images.is_empty(), "should find at least 1 image");
    }

    // =====================================================
    // 3. FigureElem — 能否查到 Figure、拿到 caption 和 label？
    // =====================================================
    #[test]
    fn introspector_finds_figures_with_caption() {
        use typst_library::model::FigureElem;

        let doc = compile_html();
        let figures = doc.introspector.query(&FigureElem::ELEM.select());

        println!("\n=== FIGURES ({}) ===", figures.len());
        for (i, fig_content) in figures.iter().enumerate() {
            let fig = fig_content.to_packed::<FigureElem>().unwrap();
            let caption = fig.caption.as_option();
            let body_func = fig.body.func().name();
            println!("[{i}] body_func={body_func} caption={caption:?}");
            // Check if figure has a label
            let label = fig_content.label();
            println!("     label={label:?}");
        }

        assert!(figures.len() >= 2, "should find at least 2 figures (image + table)");
    }

    // =====================================================
    // 4. RefElem — 能否查到交叉引用、拿到 target label？
    // =====================================================
    #[test]
    fn introspector_finds_refs_with_target() {
        use typst_library::model::RefElem;

        let doc = compile_html();
        let refs = doc.introspector.query(&RefElem::ELEM.select());

        println!("\n=== REFS ({}) ===", refs.len());
        for (i, ref_content) in refs.iter().enumerate() {
            let r = ref_content.to_packed::<RefElem>().unwrap();
            let target = &r.target;
            println!("[{i}] target={target:?}");
        }

        assert!(refs.len() >= 3, "should find at least 3 refs (@intro, @fig-demo, @tbl-vars)");
    }

    // =====================================================
    // 5. FootnoteElem — 能否查到脚注、拿到内容？
    // =====================================================
    #[test]
    fn introspector_finds_footnotes_with_content() {
        use typst_library::model::FootnoteElem;

        let doc = compile_html();
        let footnotes = doc.introspector.query(&FootnoteElem::ELEM.select());

        println!("\n=== FOOTNOTES ({}) ===", footnotes.len());
        for (i, fn_content) in footnotes.iter().enumerate() {
            let f = fn_content.to_packed::<FootnoteElem>().unwrap();
            println!("[{i}] body={:?}", f.body);
        }

        assert!(!footnotes.is_empty(), "should find at least 1 footnote");
    }

    // =====================================================
    // 6. EquationElem — 确认公式查询仍然工作
    // =====================================================
    #[test]
    fn introspector_finds_equations() {
        use typst_library::math::EquationElem;

        let doc = compile_html();
        let equations = doc.introspector.query(&EquationElem::ELEM.select());

        println!("\n=== EQUATIONS ({}) ===", equations.len());
        for (i, eq) in equations.iter().enumerate() {
            let e = eq.to_packed::<EquationElem>().unwrap();
            let is_block = *e.block.as_option().as_ref().unwrap_or(&false);
            println!("[{i}] block={is_block}");
        }

        assert!(equations.len() >= 2, "should find inline + block equation");
    }

    // =====================================================
    // 7. Tag 序列 — HtmlDocument 中的 Tag 是否按文档顺序出现？
    // =====================================================
    #[test]
    fn html_tags_appear_in_document_order() {
        let doc = compile_html();
        let body = find_body(&doc.root).unwrap_or(&doc.root);

        let mut tag_names = Vec::new();
        collect_tag_names(&body.children, &mut tag_names);

        println!("\n=== TAG SEQUENCE ({} tags) ===", tag_names.len());
        for (i, name) in tag_names.iter().enumerate() {
            println!("[{i:3}] {name}");
        }

        assert!(!tag_names.is_empty(), "should find tags in HTML tree");
        // Verify key element types appear
        let has_heading = tag_names.iter().any(|t| t.contains("heading"));
        let has_figure = tag_names.iter().any(|t| t.contains("figure"));
        let has_footnote = tag_names.iter().any(|t| t.contains("footnote"));
        let has_equation = tag_names.iter().any(|t| t.contains("equation"));
        let has_ref = tag_names.iter().any(|t| t.contains("ref"));

        println!("\nheading={has_heading} figure={has_figure} footnote={has_footnote} equation={has_equation} ref={has_ref}");
    }

    // =====================================================
    // 8. Tag ↔ Introspector 关联 — 能否通过 Tag 找到对应的 Content？
    // =====================================================
    #[test]
    fn tag_location_matches_introspector_query() {
        use typst_library::model::HeadingElem;

        let doc = compile_html();
        let body = find_body(&doc.root).unwrap_or(&doc.root);

        // Find first heading Tag in HTML tree
        let mut first_heading_tag_loc = None;
        find_first_tag_location(&body.children, "heading", &mut first_heading_tag_loc);

        if let Some(loc) = first_heading_tag_loc {
            // Try to query the introspector with this location
            let result = doc.introspector.query_first(
                &typst::foundations::Selector::Location(loc)
            );
            println!("\n=== TAG→INTROSPECTOR LOOKUP ===");
            println!("Location: {loc:?}");
            if let Some(content) = &result {
                let heading = content.to_packed::<HeadingElem>();
                println!("Found: heading={}", heading.is_some());
                if let Some(h) = heading {
                    println!("Body: {:?}", h.body);
                }
            } else {
                println!("No result from introspector for this location");
            }
            assert!(result.is_some(), "should find content via Tag location");
        } else {
            println!("No heading tag found in HTML tree");
        }
    }

    // =====================================================
    // 9. PagedDocument — FrameItem::Image 能否拿到图片 bytes？
    // =====================================================
    #[test]
    fn paged_document_has_image_with_bytes() {
        use typst::layout::PagedDocument;

        let world = TyportWorld::new(Path::new(FIXTURE)).unwrap();
        let paged = typst::compile::<PagedDocument>(&world).output.unwrap();

        let mut image_count = 0;
        let mut image_bytes_len = 0;

        for page in &paged.pages {
            collect_images(&page.frame, &mut image_count, &mut image_bytes_len);
        }

        println!("\n=== PAGED IMAGES ===");
        println!("count={image_count} total_bytes={image_bytes_len}");

        assert!(image_count >= 1, "should find at least 1 image in PagedDocument");
        assert!(image_bytes_len > 0, "image should have non-zero bytes");
    }

    fn collect_images(frame: &typst::layout::Frame, count: &mut usize, bytes: &mut usize) {
        for (_, item) in frame.items() {
            match item {
                typst::layout::FrameItem::Image(img, size, _) => {
                    *count += 1;
                    match img.kind() {
                        typst_library::visualize::ImageKind::Raster(r) => {
                            *bytes += r.data().len();
                            println!("  Raster: {}x{} format={:?} bytes={}",
                                r.width(), r.height(), r.format(), r.data().len());
                        }
                        typst_library::visualize::ImageKind::Svg(s) => {
                            println!("  SVG: bytes={}", s.data().len());
                            *bytes += s.data().len();
                        }
                        _ => {
                            println!("  Other image kind");
                        }
                    }
                    println!("  size={:?}x{:?}", size.x, size.y);
                }
                typst::layout::FrameItem::Group(g) => {
                    collect_images(&g.frame, count, bytes);
                }
                _ => {}
            }
        }
    }

    // =====================================================
    // Helpers
    // =====================================================

    fn find_body(root: &typst_html::HtmlElement) -> Option<&typst_html::HtmlElement> {
        for child in &root.children {
            if let HtmlNode::Element(elem) = child {
                let tag = format!("{}", elem.tag);
                if tag.contains("body") {
                    return Some(elem);
                }
                if let Some(found) = find_body(elem) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn collect_tag_names(children: &[HtmlNode], names: &mut Vec<String>) {
        for child in children {
            match child {
                HtmlNode::Tag(tag) => {
                    if let Tag::Start(content, _) = tag {
                        names.push(format!("Start({})", content.elem().name()));
                    } else if let Tag::End(..) = tag {
                        names.push("End".to_string());
                    }
                }
                HtmlNode::Element(elem) => {
                    collect_tag_names(&elem.children, names);
                }
                _ => {}
            }
        }
    }

    fn find_first_tag_location(
        children: &[HtmlNode],
        elem_name: &str,
        result: &mut Option<typst::introspection::Location>,
    ) {
        for child in children {
            if result.is_some() {
                return;
            }
            match child {
                HtmlNode::Tag(tag) => {
                    if let Tag::Start(content, _) = tag {
                        if content.elem().name() == elem_name {
                            *result = Some(tag.location());
                        }
                    }
                }
                HtmlNode::Element(elem) => {
                    find_first_tag_location(&elem.children, elem_name, result);
                }
                _ => {}
            }
        }
    }
}
