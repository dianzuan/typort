use typort_ooxml::document::{Document, Paragraph, ParagraphStyle, Run};
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};

use crate::world::TyportWorld;

/// Convert a Typst source file to an OOXML `Document` via the `HtmlDocument` semantic DOM.
///
/// # Errors
/// Returns compilation errors if the Typst source cannot be compiled.
pub fn convert_html(world: &TyportWorld) -> Result<Document, Vec<String>> {
    let result = typst::compile::<HtmlDocument>(world);
    let html_doc = match result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };

    let mut doc = Document::new();
    let body = find_body(&html_doc.root).unwrap_or(&html_doc.root);
    convert_block_children(&body.children, &mut doc);
    Ok(doc)
}

fn find_body(root: &HtmlElement) -> Option<&HtmlElement> {
    for child in &root.children {
        if let HtmlNode::Element(elem) = child {
            let tag = tag_name(elem);
            if tag == "body" {
                return Some(elem);
            }
            if let Some(found) = find_body(elem) {
                return Some(found);
            }
        }
    }
    None
}

fn tag_name(elem: &HtmlElement) -> String {
    let raw = format!("{}", elem.tag);
    raw.trim_matches('<').trim_matches('>').to_string()
}

fn convert_block_children(children: &[HtmlNode], doc: &mut Document) {
    for child in children {
        match child {
            HtmlNode::Element(elem) => convert_element(elem, doc),
            HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
            HtmlNode::Text(text, _) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let mut para = Paragraph::new();
                    para.runs.push(Run::new(trimmed));
                    doc.add_paragraph(para);
                }
            }
        }
    }
}

fn convert_element(elem: &HtmlElement, doc: &mut Document) {
    let tag = tag_name(elem);
    match tag.as_str() {
        "h2" => convert_heading(elem, doc, 1),
        "h3" => convert_heading(elem, doc, 2),
        "h4" => convert_heading(elem, doc, 3),
        "h5" => convert_heading(elem, doc, 4),
        "h6" => convert_heading(elem, doc, 5),
        "p" => convert_paragraph(elem, doc),
        "ol" | "ul" => convert_list(elem, doc),
        "table" => convert_table(elem, doc),
        _ => convert_block_children(&elem.children, doc),
    }
}

fn convert_heading(elem: &HtmlElement, doc: &mut Document, level: u8) {
    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level));
    collect_runs(&elem.children, &mut para.runs, false, false);
    doc.add_paragraph(para);
}

fn convert_paragraph(elem: &HtmlElement, doc: &mut Document) {
    let mut para = Paragraph::new();
    collect_runs(&elem.children, &mut para.runs, false, false);
    if !para.runs.is_empty() {
        doc.add_paragraph(para);
    }
}

fn convert_list(elem: &HtmlElement, doc: &mut Document) {
    for child in &elem.children {
        if let HtmlNode::Element(li) = child
            && tag_name(li) == "li"
        {
            let mut para = Paragraph::new();
            collect_runs(&li.children, &mut para.runs, false, false);
            if !para.runs.is_empty() {
                doc.add_paragraph(para);
            }
        }
    }
}

fn convert_table(elem: &HtmlElement, doc: &mut Document) {
    for child in &elem.children {
        if let HtmlNode::Element(row_or_section) = child {
            let tag = tag_name(row_or_section);
            if tag == "tr" {
                convert_table_row(row_or_section, doc);
            } else if tag == "thead" || tag == "tbody" || tag == "tfoot" {
                for inner in &row_or_section.children {
                    if let HtmlNode::Element(tr) = inner
                        && tag_name(tr) == "tr"
                    {
                        convert_table_row(tr, doc);
                    }
                }
            }
        }
    }
}

fn convert_table_row(tr: &HtmlElement, doc: &mut Document) {
    let mut para = Paragraph::new();
    for (i, cell) in tr.children.iter().enumerate() {
        if let HtmlNode::Element(td) = cell {
            let tag = tag_name(td);
            if tag == "td" || tag == "th" {
                if i > 0 {
                    para.runs.push(Run::new("\t"));
                }
                collect_runs(&td.children, &mut para.runs, tag == "th", false);
            }
        }
    }
    if !para.runs.is_empty() {
        doc.add_paragraph(para);
    }
}

fn collect_runs(children: &[HtmlNode], runs: &mut Vec<Run>, bold: bool, italic: bool) {
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                if !text.is_empty() {
                    let mut run = Run::new(text.as_str());
                    run.bold = bold;
                    run.italic = italic;
                    runs.push(run);
                }
            }
            HtmlNode::Element(elem) => {
                let tag = tag_name(elem);
                let new_bold = bold || tag == "strong" || tag == "b";
                let new_italic = italic || tag == "em" || tag == "i";
                collect_runs(&elem.children, runs, new_bold, new_italic);
            }
            HtmlNode::Frame(_) | HtmlNode::Tag(_) => {}
        }
    }
}

/// Legacy Phase 0 converter (frame-based text extraction).
pub mod legacy {
    use typort_ooxml::document::{Document, Paragraph};
    use typst::layout::{Frame, FrameItem, PagedDocument};

    #[must_use]
    pub fn convert_document(paged: &PagedDocument) -> Document {
        let mut doc = Document::new();
        for page in &paged.pages {
            let mut text_buf = String::new();
            extract_text_from_frame(&page.frame, &mut text_buf);
            if !text_buf.is_empty() {
                let mut para = Paragraph::new();
                para.add_run(&text_buf);
                doc.add_paragraph(para);
            }
        }
        doc
    }

    fn extract_text_from_frame(frame: &Frame, buf: &mut String) {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Text(text_item) => buf.push_str(&text_item.text),
                FrameItem::Group(group) => extract_text_from_frame(&group.frame, buf),
                _ => {}
            }
        }
    }
}
