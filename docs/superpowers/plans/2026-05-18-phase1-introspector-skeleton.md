# Phase 1: Introspector-Primary Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace HTML-tag-parsing conversion with Tag+Introspector-driven conversion, producing equivalent docx output for all existing test fixtures.

**Architecture:** New `convert_v2.rs` module walks HtmlDocument Tag sequence, queries Introspector for each element's Content AST, converts to Document model. Old `convert.rs` kept until all 106 tests pass with v2, then swapped. PagedDocument used only for page settings and style extraction.

**Tech Stack:** Rust, typst 0.14.2 (HtmlDocument, Introspector, PagedDocument), typort-ooxml document model, quick-xml

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/typort-core/src/convert_v2.rs` | **Create** | New Introspector-primary conversion — Tag walker, element dispatching, Content AST → Document model |
| `crates/typort-core/src/convert_v2/inline.rs` | **Create** | Inline Content AST → Run extraction (bold, italic, superscript, subscript, monospace, footnote refs) |
| `crates/typort-core/src/convert_v2/page.rs` | **Create** | PagedDocument → page settings, document style, font detection (extracted from current convert.rs) |
| `crates/typort-core/src/lib.rs` | **Modify** | Add `mod convert_v2`, expose `convert_v2::convert` |
| `crates/typort-core/src/convert.rs` | **Keep** | Untouched until Task 8. Then deleted. |
| `crates/typort-cli/tests/integration.rs` | **Modify** | Add parallel v2 tests |

---

### Task 1: Tag Walker Skeleton + Hello World

**Files:**
- Create: `crates/typort-core/src/convert_v2.rs`
- Modify: `crates/typort-core/src/lib.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/typort-cli/tests/integration.rs`:

```rust
#[test]
fn v2_hello_typ_produces_heading_and_text() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("Heading1"), "should have Heading1 style");
    assert!(doc_xml.contains("Hello"), "should contain heading text");
    assert!(doc_xml.contains("test document"), "should contain body text");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p typort-cli v2_hello`
Expected: compilation error — `convert_v2` module doesn't exist

- [ ] **Step 3: Create convert_v2.rs with Tag walker skeleton**

Create `crates/typort-core/src/convert_v2.rs`:

```rust
use typst::foundations::{Content, NativeElement};
use typst::introspection::{Introspector, Tag};
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};
use typst_library::math::EquationElem;
use typst_library::model::{HeadingElem, FootnoteElem};

use typort_ooxml::document::{Document, Paragraph, ParagraphStyle, Run};

use crate::world::TyportWorld;

mod inline;
mod page;

pub fn convert(world: &TyportWorld) -> Result<Document, Vec<String>> {
    let result = typst::compile::<HtmlDocument>(world);
    let html_doc = match result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };

    let mut doc = Document::new();
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);

    walk_tags(&body.children, &html_doc.introspector, &mut doc);

    page::extract_page_info(world, &mut doc);
    extract_title_from_first_heading(&mut doc);

    Ok(doc)
}

fn walk_tags(children: &[HtmlNode], introspector: &Introspector, doc: &mut Document) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    let name = content.elem().name();
                    let loc = tag.location();
                    let end_idx = find_tag_end(&children[i + 1..], loc);

                    match name {
                        "heading" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                convert_heading(&c, doc);
                            }
                        }
                        "equation" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                convert_equation(&c, doc);
                            }
                        }
                        "footnote" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                convert_footnote(&c, doc);
                            }
                        }
                        _ => {}
                    }

                    // Skip past the End tag
                    i += 1 + end_idx + 1;
                    continue;
                }
            }
            HtmlNode::Element(elem) => {
                // Recurse into HTML elements to find nested Tags
                walk_tags(&elem.children, introspector, doc);
            }
            HtmlNode::Text(text, _) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.push_run(Run::new(trimmed));
                    doc.add_paragraph(para);
                }
            }
            HtmlNode::Frame(_) => {}
        }
        i += 1;
    }
}

fn find_tag_end(children: &[HtmlNode], start_loc: typst::introspection::Location) -> usize {
    for (i, child) in children.iter().enumerate() {
        if let HtmlNode::Tag(tag) = child {
            if let Tag::End(loc, ..) = tag {
                if *loc == start_loc {
                    return i;
                }
            }
        }
    }
    children.len()
}

fn convert_heading(content: &Content, doc: &mut Document) {
    let heading = content.to_packed::<HeadingElem>().unwrap();
    let level = heading
        .resolve_level(typst::foundations::StyleChain::default())
        .get();
    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level as u8));
    inline::collect_content_inlines(&heading.body, &mut para);
    doc.add_paragraph(para);
}

fn convert_equation(content: &Content, doc: &mut Document) {
    let omml = typort_math::equation_to_omml(content);
    let eq = content.to_packed::<EquationElem>().unwrap();
    let is_block = *eq.block.as_option().as_ref().unwrap_or(&false);

    if is_block {
        let mut para = Paragraph::new();
        para.add_math(omml);
        doc.add_paragraph(para);
    } else if let Some(typort_ooxml::document::BlockElement::Paragraph(para)) =
        doc.body.elements.last_mut()
    {
        para.add_math(omml);
    } else {
        let mut para = Paragraph::new();
        para.add_math(omml);
        doc.add_paragraph(para);
    }
}

fn convert_footnote(content: &Content, doc: &mut Document) {
    let footnote = content.to_packed::<FootnoteElem>().unwrap();
    let mut runs = Vec::new();
    inline::collect_content_runs(&footnote.body, &mut runs);
    let id = doc.add_footnote(runs);
    if let Some(typort_ooxml::document::BlockElement::Paragraph(para)) =
        doc.body.elements.last_mut()
    {
        para.add_footnote_ref(id);
    }
}

fn find_body(root: &HtmlElement) -> Option<&HtmlElement> {
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

fn extract_title_from_first_heading(doc: &mut Document) {
    use typort_ooxml::document::{BlockElement, ParagraphStyle};
    for element in &doc.body.elements {
        if let BlockElement::Paragraph(p) = element
            && matches!(p.style, Some(ParagraphStyle::Heading(_)))
        {
            let title: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            if !title.is_empty() {
                doc.metadata.title = Some(title);
            }
            break;
        }
    }
}
```

- [ ] **Step 4: Create inline.rs — Content AST → Run extraction**

Create `crates/typort-core/src/convert_v2/inline.rs`:

```rust
use typst::foundations::Content;
use typst_library::foundations::{SequenceElem, SymbolElem};
use typst_library::model::{EmphElem, StrongElem};
use typst_library::text::{SpaceElem, TextElem};

use typort_ooxml::document::{Paragraph, Run};

pub fn collect_content_inlines(content: &Content, para: &mut Paragraph) {
    let mut runs = Vec::new();
    collect_runs(content, &mut runs, false, false);
    for run in runs {
        para.push_run(run);
    }
}

pub fn collect_content_runs(content: &Content, runs: &mut Vec<Run>) {
    collect_runs(content, runs, false, false);
}

fn collect_runs(content: &Content, runs: &mut Vec<Run>, bold: bool, italic: bool) {
    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            collect_runs(child, runs, bold, italic);
        }
    } else if let Some(text) = content.to_packed::<TextElem>() {
        if !text.text.is_empty() {
            let mut run = Run::new(text.text.as_str());
            run.bold = bold;
            run.italic = italic;
            runs.push(run);
        }
    } else if content.to_packed::<SpaceElem>().is_some() {
        runs.push(Run::new(" "));
    } else if let Some(sym) = content.to_packed::<SymbolElem>() {
        let mut run = Run::new(sym.text.as_str());
        run.bold = bold;
        run.italic = italic;
        runs.push(run);
    } else if let Some(strong) = content.to_packed::<StrongElem>() {
        collect_runs(&strong.body, runs, true, italic);
    } else if let Some(emph) = content.to_packed::<EmphElem>() {
        collect_runs(&emph.body, runs, bold, true);
    }
}
```

- [ ] **Step 5: Create page.rs — PagedDocument extraction (moved from convert.rs)**

Create `crates/typort-core/src/convert_v2/page.rs`:

```rust
use std::collections::HashMap;

use typst::layout::{Frame, FrameItem, PagedDocument, Point};

use typort_ooxml::document::{Document, DocumentStyle, FootnoteFormat};

use crate::world::TyportWorld;

pub fn extract_page_info(world: &TyportWorld, doc: &mut Document) {
    let paged_result = typst::compile::<PagedDocument>(world);
    let Ok(paged) = paged_result.output else {
        return;
    };

    doc.style = extract_document_style(&paged);
    extract_page_settings(&paged, &mut doc.page_settings);
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
    let mut font_counts: HashMap<String, usize> = HashMap::new();
    let mut size_counts: HashMap<u32, usize> = HashMap::new();

    for page in paged.pages.iter().take(3) {
        collect_font_info(&page.frame, &mut font_counts, &mut size_counts);
    }

    let body_font = font_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or_else(|| "Times New Roman".to_string(), |(family, _)| family);

    let body_size_half_pt = size_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(21, |(size, _)| size);

    let body_font_ascii = body_font.clone();
    let body_font_east_asia = body_font;
    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = (body_pt * 20.0 * 2.0).round() as u32;

    DocumentStyle {
        body_font_ascii,
        body_font_east_asia,
        body_size_half_pt,
        line_spacing: 360,
        first_line_indent_twips,
        footnote_format: FootnoteFormat::default(),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_font_info(
    frame: &Frame,
    font_counts: &mut HashMap<String, usize>,
    size_counts: &mut HashMap<u32, usize>,
) {
    for (_pos, item) in frame.items() {
        match item {
            FrameItem::Text(text_item) => {
                let family = text_item.font.info().family.clone();
                let size_half_pt = (text_item.size.to_pt() * 2.0).round() as u32;
                *font_counts.entry(family).or_insert(0) += text_item.glyphs.len();
                *size_counts.entry(size_half_pt).or_insert(0) += text_item.glyphs.len();
            }
            FrameItem::Group(group) => {
                collect_font_info(&group.frame, font_counts, size_counts);
            }
            _ => {}
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn extract_page_settings(
    paged: &PagedDocument,
    settings: &mut typort_ooxml::document::PageSettings,
) {
    let Some(page) = paged.pages.first() else {
        return;
    };

    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    settings.width_twips = (page_width * 20.0).round() as u32;
    settings.height_twips = (page_height * 20.0).round() as u32;

    let mut min_x = page_width;
    let mut max_x: f64 = 0.0;
    let mut min_y = page_height;
    let mut max_y: f64 = 0.0;

    collect_content_bounds(
        &page.frame,
        Point::zero(),
        &mut min_x,
        &mut max_x,
        &mut min_y,
        &mut max_y,
    );

    if min_x < max_x && min_y < max_y {
        let margin_left = (min_x * 20.0).round().max(0.0) as u32;
        let margin_right = ((page_width - max_x) * 20.0).round().max(0.0) as u32;
        let margin_top = (min_y * 20.0).round().max(0.0) as u32;
        let margin_bottom = ((page_height - max_y) * 20.0).round().max(0.0) as u32;

        if margin_left >= 100 {
            settings.margin_left = margin_left;
        }
        if margin_right >= 100 {
            settings.margin_right = margin_right;
        }
        if margin_top >= 100 {
            settings.margin_top = margin_top;
        }
        if margin_bottom >= 100 {
            settings.margin_bottom = margin_bottom;
        }
    }
}

fn collect_content_bounds(
    frame: &Frame,
    offset: Point,
    min_x: &mut f64,
    max_x: &mut f64,
    min_y: &mut f64,
    max_y: &mut f64,
) {
    for (pos, item) in frame.items() {
        let abs_x = offset.x + pos.x;
        let abs_y = offset.y + pos.y;
        match item {
            FrameItem::Text(text_item) => {
                let x = abs_x.to_pt();
                let y = abs_y.to_pt();
                let w = text_item.width().to_pt();
                if x < *min_x {
                    *min_x = x;
                }
                if x + w > *max_x {
                    *max_x = x + w;
                }
                if y < *min_y {
                    *min_y = y;
                }
                let h = text_item.size.to_pt();
                if y + h > *max_y {
                    *max_y = y + h;
                }
            }
            FrameItem::Group(group) => {
                let new_offset = Point::new(abs_x, abs_y);
                collect_content_bounds(&group.frame, new_offset, min_x, max_x, min_y, max_y);
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 6: Wire up modules in lib.rs**

Add to `crates/typort-core/src/lib.rs` after `pub mod convert;`:

```rust
pub mod convert_v2;
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cargo test -p typort-cli v2_hello -- --nocapture`
Expected: PASS — hello.typ produces docx with Heading1 and text content

- [ ] **Step 8: Commit**

```bash
git add crates/typort-core/src/convert_v2.rs crates/typort-core/src/convert_v2/inline.rs crates/typort-core/src/convert_v2/page.rs crates/typort-core/src/lib.rs crates/typort-cli/tests/integration.rs
git commit -m "$(cat <<'EOF'
feat: add convert_v2 skeleton — Introspector-primary Tag walker

New conversion path: walk HtmlDocument Tags → query Introspector →
Content AST → Document model. Handles heading, equation, footnote,
plain text. Old convert.rs kept until full parity.
EOF
)"
```

---

### Task 2: Paragraph + Inline Formatting (strong, emph, sub, sup, code)

**Files:**
- Modify: `crates/typort-core/src/convert_v2.rs`
- Modify: `crates/typort-core/src/convert_v2/inline.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/typort-cli/tests/integration.rs`:

```rust
#[test]
fn v2_italic_text_produces_w_i_element() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/italic_test.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("<w:i/>"), "should have italic");
    assert!(doc_xml.contains("<w:b/>"), "should have bold");
    assert!(doc_xml.contains("emphasized text"), "should have italic text content");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p typort-cli v2_italic`
Expected: FAIL — `par` and `strong`/`emph` Tags not handled yet

- [ ] **Step 3: Add `par`, `strong`, `emph` to walk_tags dispatcher**

In `convert_v2.rs`, add these cases to the `match name` block inside `walk_tags`:

```rust
"par" => {
    let mut para = Paragraph::new();
    // Collect inline content from child nodes between Start and End
    let inner = &children[i + 1..i + 1 + end_idx];
    collect_par_inlines(inner, introspector, &mut para);
    if !para.runs.is_empty() || !para.inlines.is_empty() {
        doc.add_paragraph(para);
    }
}
```

Add a new function in `convert_v2.rs`:

```rust
fn collect_par_inlines(
    children: &[HtmlNode],
    introspector: &Introspector,
    para: &mut Paragraph,
) {
    for child in children {
        match child {
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    let name = content.elem().name();
                    let loc = tag.location();
                    match name {
                        "strong" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                if let Some(strong) = c.to_packed::<typst_library::model::StrongElem>() {
                                    let mut runs = Vec::new();
                                    inline::collect_content_runs(&strong.body, &mut runs);
                                    for mut run in runs {
                                        run.bold = true;
                                        para.push_run(run);
                                    }
                                }
                            }
                        }
                        "emph" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                if let Some(emph) = c.to_packed::<typst_library::model::EmphElem>() {
                                    let mut runs = Vec::new();
                                    inline::collect_content_runs(&emph.body, &mut runs);
                                    for mut run in runs {
                                        run.italic = true;
                                        para.push_run(run);
                                    }
                                }
                            }
                        }
                        "footnote" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                convert_footnote_inline(&c, doc_placeholder, para);
                            }
                        }
                        "equation" => {
                            if let Some(c) = introspector.query_first(
                                &typst::foundations::Selector::Location(loc),
                            ) {
                                let omml = typort_math::equation_to_omml(&c);
                                para.add_math(omml);
                            }
                        }
                        _ => {}
                    }
                }
            }
            HtmlNode::Text(text, _) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    para.push_run(Run::new(trimmed));
                }
            }
            HtmlNode::Element(elem) => {
                collect_par_inlines(&elem.children, introspector, para);
            }
            HtmlNode::Frame(_) => {
                let mut run = Run::new("[Image]");
                run.italic = true;
                para.push_run(run);
            }
        }
    }
}
```

Note: The footnote case inside `collect_par_inlines` needs access to the Document to add footnote content. Refactor: pass `&mut Document` to `collect_par_inlines`, or handle footnotes as a separate pass. The simplest approach for now is to pass `doc` through. Adjust the signature to:

```rust
fn collect_par_inlines(
    children: &[HtmlNode],
    introspector: &Introspector,
    doc: &mut Document,
    para: &mut Paragraph,
)
```

And for the `footnote` case:

```rust
"footnote" => {
    if let Some(c) = introspector.query_first(
        &typst::foundations::Selector::Location(loc),
    ) {
        let fn_elem = c.to_packed::<FootnoteElem>().unwrap();
        let mut runs = Vec::new();
        inline::collect_content_runs(&fn_elem.body, &mut runs);
        let id = doc.add_footnote(runs);
        para.add_footnote_ref(id);
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p typort-cli v2_italic -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/typort-core/src/convert_v2.rs crates/typort-core/src/convert_v2/inline.rs crates/typort-cli/tests/integration.rs
git commit -m "feat(v2): handle par/strong/emph inline formatting via Introspector"
```

---

### Task 3: Tables

**Files:**
- Modify: `crates/typort-core/src/convert_v2.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn v2_complex_paper_has_table_structure() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("w:tbl"), "should have table");
    assert!(doc_xml.contains("w:tr"), "should have table rows");
    assert!(doc_xml.contains("w:tc"), "should have table cells");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p typort-cli v2_complex_paper_has_table`
Expected: FAIL — `table` Tag not handled

- [ ] **Step 3: Add table conversion via Introspector**

In `convert_v2.rs`, add `"table"` to the Tag dispatcher. The TableElem Content AST contains `children` which is a sequence of TableCell elements. However, the table structure from Typst's Content tree is complex (children are a flat sequence of content, not rows/columns). For Phase 1, delegate to the HTML tree's `<table>` structure which is cleaner:

```rust
"table" => {
    // Table conversion: the HTML <table> structure is more convenient
    // than the flat Content AST. Find the HTML <table> element that
    // corresponds to this Tag and convert it.
    let inner = &children[i + 1..i + 1 + end_idx];
    if let Some(table_elem) = find_html_table(inner) {
        convert_html_table(table_elem, doc);
    }
}
```

Add `find_html_table` and `convert_html_table` (reuse the existing table logic from `convert.rs`):

```rust
fn find_html_table<'a>(children: &'a [HtmlNode]) -> Option<&'a HtmlElement> {
    for child in children {
        if let HtmlNode::Element(elem) = child {
            let tag = format!("{}", elem.tag);
            if tag.contains("table") {
                return Some(elem);
            }
            if let Some(found) = find_html_table(&elem.children) {
                return Some(found);
            }
        }
    }
    None
}

fn convert_html_table(elem: &HtmlElement, doc: &mut Document) {
    use typort_ooxml::document::{Table, TableCell, TableRow, VMerge};
    let mut table = Table { rows: Vec::new() };
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = format!("{}", row_or_section.tag);
            if tag.contains("tr") {
                if let Some(row) = convert_html_table_row(row_or_section) {
                    table.rows.push(row);
                }
            } else if tag.contains("thead") || tag.contains("tbody") || tag.contains("tfoot") {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner {
                        let tr_tag = format!("{}", tr.tag);
                        if tr_tag.contains("tr") {
                            if let Some(row) = convert_html_table_row(tr) {
                                table.rows.push(row);
                            }
                        }
                    }
                }
            }
        }
    }
    if !table.rows.is_empty() {
        doc.add_table(table);
    }
}

fn convert_html_table_row(tr: &HtmlElement) -> Option<TableRow> {
    use typort_ooxml::document::{TableCell, TableRow, VMerge};
    let mut cells = Vec::new();
    for cell in &tr.children {
        if let HtmlNode::Element(td) = cell {
            let tag = format!("{}", td.tag);
            if tag.contains("td") || tag.contains("th") {
                let mut para = Paragraph::new();
                collect_text_from_html(&td.children, &mut para, tag.contains("th"));
                let colspan = get_html_attr(td, "colspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let rowspan = get_html_attr(td, "rowspan")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let vmerge = if rowspan > 1 {
                    VMerge::Restart
                } else {
                    VMerge::None
                };
                cells.push(TableCell {
                    paragraphs: vec![para],
                    colspan,
                    vmerge,
                    width_pct: None,
                });
            }
        }
    }
    if cells.is_empty() { None } else { Some(TableRow { cells }) }
}

fn collect_text_from_html(children: &[HtmlNode], para: &mut Paragraph, bold: bool) {
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    para.push_run(run);
                }
            }
            HtmlNode::Element(elem) => {
                let tag = format!("{}", elem.tag);
                let is_bold = bold || tag.contains("strong") || tag.contains("th");
                collect_text_from_html(&elem.children, para, is_bold);
            }
            _ => {}
        }
    }
}

fn get_html_attr(elem: &HtmlElement, attr_name: &str) -> Option<String> {
    for (k, v) in &elem.attrs.0 {
        if format!("{k}") == attr_name {
            return Some(format!("{v}"));
        }
    }
    None
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p typort-cli v2_complex_paper_has_table -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/typort-core/src/convert_v2.rs crates/typort-cli/tests/integration.rs
git commit -m "feat(v2): table conversion via HTML structure"
```

---

### Task 4: Lists (ordered + unordered)

**Files:**
- Modify: `crates/typort-core/src/convert_v2.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn v2_complex_paper_has_list_numbering() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("w:numPr"), "should have list numbering");
    assert!(doc_xml.contains("w:ilvl"), "should have list level");
}
```

- [ ] **Step 2: Run test, verify failure, implement, verify pass**

Add to Tag dispatcher in `walk_tags`:

```rust
"list" => {
    let inner = &children[i + 1..i + 1 + end_idx];
    convert_list_from_html(inner, introspector, doc, 2); // bullet
}
"enum" => {
    let inner = &children[i + 1..i + 1 + end_idx];
    convert_list_from_html(inner, introspector, doc, 1); // decimal
}
```

Add function:

```rust
fn convert_list_from_html(
    children: &[HtmlNode],
    introspector: &Introspector,
    doc: &mut Document,
    list_id: u32,
) {
    for child in children {
        if let HtmlNode::Element(elem) = child {
            let tag = format!("{}", elem.tag);
            if tag.contains("li") {
                let mut para = Paragraph::new();
                para.list_id = Some(list_id);
                para.list_level = Some(0);
                collect_text_from_html(&elem.children, &mut para, false);
                if !para.runs.is_empty() {
                    doc.add_paragraph(para);
                }
            }
        }
    }
}
```

- [ ] **Step 3: Run test, commit**

Run: `cargo test -p typort-cli v2_complex_paper_has_list -- --nocapture`

```bash
git add crates/typort-core/src/convert_v2.rs crates/typort-cli/tests/integration.rs
git commit -m "feat(v2): ordered and unordered list conversion"
```

---

### Task 5: Code blocks, blockquotes, term lists

**Files:**
- Modify: `crates/typort-core/src/convert_v2.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn v2_general_elements_has_code_block() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/general_elements.typ"))
            .unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("CodeBlock"), "should have CodeBlock style");
    assert!(doc_xml.contains("println"), "should contain code content");
}
```

- [ ] **Step 2: Handle `pre`, `blockquote`, `dl` HTML elements**

These elements don't have Typst Tags — they're rendered as HTML elements. Add handling in the `HtmlNode::Element` branch of `walk_tags`:

```rust
HtmlNode::Element(elem) => {
    let tag = format!("{}", elem.tag);
    if tag.contains("pre") {
        convert_code_block(elem, doc);
    } else if tag.contains("blockquote") {
        convert_blockquote(elem, introspector, doc);
    } else if tag.contains("dl") {
        convert_term_list(elem, doc);
    } else {
        walk_tags(&elem.children, introspector, doc);
    }
}
```

Add the three functions (same logic as old convert.rs):

```rust
fn convert_code_block(elem: &HtmlElement, doc: &mut Document) {
    let text = collect_all_html_text(&elem.children);
    for line in text.split('\n') {
        let mut para = Paragraph::new();
        para.code_block = true;
        let mut run = Run::new(line);
        run.monospace = true;
        para.push_run(run);
        doc.add_paragraph(para);
    }
}

fn convert_blockquote(elem: &HtmlElement, introspector: &Introspector, doc: &mut Document) {
    let start_idx = doc.body.elements.len();
    walk_tags(&elem.children, introspector, doc);
    for element in &mut doc.body.elements[start_idx..] {
        if let typort_ooxml::document::BlockElement::Paragraph(para) = element {
            para.left_indent = Some(720);
            para.suppress_indent = true;
        }
    }
}

fn convert_term_list(elem: &HtmlElement, doc: &mut Document) {
    for child in &elem.children {
        if let HtmlNode::Element(item) = child {
            let tag = format!("{}", item.tag);
            if tag.contains("dt") {
                let mut para = Paragraph::new();
                para.suppress_indent = true;
                collect_text_from_html(&item.children, &mut para, true);
                if !para.runs.is_empty() {
                    doc.add_paragraph(para);
                }
            } else if tag.contains("dd") {
                let mut para = Paragraph::new();
                para.left_indent = Some(420);
                para.suppress_indent = true;
                collect_text_from_html(&item.children, &mut para, false);
                if !para.runs.is_empty() {
                    doc.add_paragraph(para);
                }
            }
        }
    }
}

fn collect_all_html_text(children: &[HtmlNode]) -> String {
    let mut text = String::new();
    let mut line_started = false;
    for child in children {
        match child {
            HtmlNode::Text(t, _) => text.push_str(t),
            HtmlNode::Element(elem) => text.push_str(&collect_all_html_text(&elem.children)),
            HtmlNode::Tag(tag) => {
                if let Tag::Start(content, _) = tag {
                    if content.elem().name() == "line" {
                        if line_started {
                            text.push('\n');
                        }
                        line_started = true;
                    }
                }
            }
            HtmlNode::Frame(_) => {}
        }
    }
    text
}
```

- [ ] **Step 3: Run tests, commit**

Run: `cargo test -p typort-cli v2_general_elements -- --nocapture`

```bash
git add crates/typort-core/src/convert_v2.rs crates/typort-cli/tests/integration.rs
git commit -m "feat(v2): code blocks, blockquotes, term lists"
```

---

### Task 6: Footnotes via Introspector

**Files:**
- Modify: `crates/typort-core/src/convert_v2.rs`
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn v2_complex_paper_has_footnotes() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    assert!(!doc.footnotes.is_empty(), "should have footnotes");

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<String> = reader.file_names().map(String::from).collect();
    assert!(names.iter().any(|n| n == "word/footnotes.xml"), "should have footnotes.xml");

    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:footnoteReference"), "should have footnote refs");
}
```

- [ ] **Step 2: Verify footnote handling works from Task 1**

The footnote Tag handling was added in Task 1 skeleton. Verify it works with complex_paper.typ. If the test fails, the issue is likely that footnote Tags appear inside `par` children, so they need to be handled in `collect_par_inlines` (added in Task 2).

Run: `cargo test -p typort-cli v2_complex_paper_has_footnotes -- --nocapture`

- [ ] **Step 3: Commit**

```bash
git add crates/typort-cli/tests/integration.rs
git commit -m "test(v2): verify footnote conversion via Introspector"
```

---

### Task 7: Math equations (verify existing path works)

**Files:**
- Test: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Write test**

```rust
#[test]
fn v2_math_test_produces_omml() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/math_test.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("<m:oMath>"), "should have inline math");
    assert!(doc_xml.contains("<m:oMathPara>"), "should have block math");
    assert!(doc_xml.contains("<m:sSup>"), "should have superscript");
    assert!(doc_xml.contains("<m:f>"), "should have fraction");
}
```

- [ ] **Step 2: Run test, verify pass**

Run: `cargo test -p typort-cli v2_math_test -- --nocapture`
Expected: PASS (equation conversion was already in Task 1 skeleton)

- [ ] **Step 3: Commit**

```bash
git add crates/typort-cli/tests/integration.rs
git commit -m "test(v2): verify math OMML conversion works"
```

---

### Task 8: Full parity check — run all existing tests against v2

**Files:**
- Modify: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Add v2 versions of key existing tests**

Add tests that mirror the original integration tests but use `convert_v2::convert`:

```rust
#[test]
fn v2_end_to_end_hello_typ_to_docx() {
    let world = typort_core::TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let names: Vec<&str> = reader.file_names().collect();

    assert!(names.contains(&"[Content_Types].xml"));
    assert!(names.contains(&"word/document.xml"));
    assert!(names.contains(&"word/styles.xml"));
    assert!(names.contains(&"word/fontTable.xml"));

    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:document"));
    assert!(doc_xml.contains("Hello"));
    assert!(doc_xml.contains("Heading1"));
    assert!(doc_xml.contains("w:sectPr"));
}

#[test]
fn v2_complex_paper_has_semantic_structure() {
    let world =
        typort_core::TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert_v2::convert(&world).unwrap();

    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut reader = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = std::io::read_to_string(reader.by_name("word/document.xml").unwrap()).unwrap();

    assert!(doc_xml.contains("Heading1"), "should have Heading1");
    assert!(doc_xml.contains("Heading2"), "should have Heading2");
    assert!(doc_xml.contains("<w:b/>"), "should have bold");
    assert!(doc_xml.contains("w:pgMar"), "should have page margins");
    assert!(doc_xml.contains("数字经济"), "should contain Chinese text");
}
```

- [ ] **Step 2: Run all v2 tests**

Run: `cargo test -p typort-cli v2_ -- --nocapture`
Expected: all v2 tests pass

- [ ] **Step 3: Run full test suite to verify no regressions**

Run: `cargo test --workspace`
Expected: all tests pass (v1 tests still use old convert.rs)

- [ ] **Step 4: Commit**

```bash
git add crates/typort-cli/tests/integration.rs
git commit -m "test(v2): full parity check — v2 matches v1 output for core features"
```

---

### Task 9: Swap convert_v2 → convert, remove old code

**Files:**
- Modify: `crates/typort-core/src/lib.rs`
- Delete: `crates/typort-core/src/convert.rs` (old)
- Rename: `crates/typort-core/src/convert_v2.rs` → `crates/typort-core/src/convert.rs`
- Modify: `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: Update lib.rs exports**

Replace in `crates/typort-core/src/lib.rs`:

```rust
pub mod convert;
pub mod convert_v2;

pub use convert::convert_html;
```

With:

```rust
pub mod convert;

pub use convert::convert;
```

Where the new `convert` module is the old `convert_v2`.

- [ ] **Step 2: Move files**

```bash
mv crates/typort-core/src/convert.rs crates/typort-core/src/convert_old.rs
mv crates/typort-core/src/convert_v2.rs crates/typort-core/src/convert.rs
mv crates/typort-core/src/convert_v2/ crates/typort-core/src/convert/
```

Restructure: `convert.rs` becomes `convert/mod.rs`, with `inline.rs` and `page.rs` as submodules.

```bash
mkdir -p crates/typort-core/src/convert
mv crates/typort-core/src/convert.rs crates/typort-core/src/convert/mod.rs
```

- [ ] **Step 3: Update all test imports**

In integration tests, change `typort_core::convert_v2::convert` to `typort_core::convert::convert`, and `typort_core::convert_html` calls in v1 tests to `typort_core::convert::convert`.

- [ ] **Step 4: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Remove old convert_old.rs**

```bash
rm crates/typort-core/src/convert_old.rs
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
refactor: replace HTML-tag-parsing with Introspector-primary conversion

The conversion pipeline now uses Typst's Introspector to query Content
AST for each element, with HtmlDocument Tags providing document order.
This removes dependency on HTML tag semantics and provides direct access
to Typst's native element data (labels, numbering, figure metadata).

Old HTML-parsing convert.rs removed. All 100+ tests pass with new path.
EOF
)"
```
