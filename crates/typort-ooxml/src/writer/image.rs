use std::io::{self, Write};

use quick_xml::Writer;

use super::DocParts;
use crate::document::{
    BlockElement, CellContent, Document, ImageData, InlineElement, Paragraph, Table,
};

/// Collect all images from the document model in order of appearance.
/// Returns a `Vec<&ImageData>` where the index corresponds to the 1-based image number.
pub(super) fn collect_images(doc: &Document) -> Vec<&ImageData> {
    let mut images = Vec::new();
    for el in &doc.body.elements {
        match el {
            BlockElement::Paragraph(p) => collect_images_from_para(p, &mut images),
            BlockElement::Table(t) => {
                collect_images_from_table(t, &mut images);
            }
            BlockElement::BibliographyBlock { paragraphs } => {
                for p in paragraphs {
                    collect_images_from_para(p, &mut images);
                }
            }
        }
    }
    images
}

fn collect_images_from_para<'a>(para: &'a Paragraph, images: &mut Vec<&'a ImageData>) {
    for inline in &para.inlines {
        if let InlineElement::Image(img) = inline {
            images.push(img);
        }
    }
}

fn collect_images_from_table<'a>(table: &'a Table, images: &mut Vec<&'a ImageData>) {
    for row in &table.rows {
        for cell in &row.cells {
            // Check structured content first (includes nested tables)
            if cell.content.is_empty() {
                for para in &cell.paragraphs {
                    collect_images_from_para(para, images);
                }
            } else {
                for item in &cell.content {
                    match item {
                        CellContent::Paragraph(p) => {
                            collect_images_from_para(p, images);
                        }
                        CellContent::Table(t) => {
                            collect_images_from_table(t, images);
                        }
                    }
                }
            }
        }
    }
}

/// Compute the relationship ID for an image given its 1-based index.
/// Image relationships follow all fixed parts in `document.xml.rels`.
pub(super) fn image_rel_id(image_index: usize, parts: &DocParts) -> String {
    format!("rId{}", parts.relationships.len() + image_index)
}

/// Write a `wp:inline` drawing element for an embedded image.
pub(super) fn write_image_inline<W: Write>(
    writer: &mut Writer<W>,
    img: &ImageData,
    n: usize,
    rid: &str,
) -> io::Result<()> {
    let cx = img.width_emu.to_string();
    let cy = img.height_emu.to_string();
    let id_str = n.to_string();
    let ext = img.format.extension();
    let name = format!("Image{n}");
    let filename = format!("image{n}.{ext}");

    writer.create_element("w:r").write_inner_content(|w| {
        w.create_element("w:drawing").write_inner_content(|dw| {
            dw.create_element("wp:inline")
                .with_attribute(("distT", "0"))
                .with_attribute(("distB", "0"))
                .with_attribute(("distL", "0"))
                .with_attribute(("distR", "0"))
                .write_inner_content(|inl| {
                    inl.create_element("wp:extent")
                        .with_attribute(("cx", cx.as_str()))
                        .with_attribute(("cy", cy.as_str()))
                        .write_empty()?;
                    inl.create_element("wp:docPr")
                        .with_attribute(("id", id_str.as_str()))
                        .with_attribute(("name", name.as_str()))
                        .write_empty()?;
                    inl.create_element("a:graphic")
                        .with_attribute(("xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main"))
                        .write_inner_content(|gr| {
                            gr.create_element("a:graphicData")
                                .with_attribute(("uri", "http://schemas.openxmlformats.org/drawingml/2006/picture"))
                                .write_inner_content(|gd| {
                                    gd.create_element("pic:pic")
                                        .with_attribute(("xmlns:pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"))
                                        .write_inner_content(|pic| {
                                            pic.create_element("pic:nvPicPr").write_inner_content(|nv| {
                                                nv.create_element("pic:cNvPr")
                                                    .with_attribute(("id", id_str.as_str()))
                                                    .with_attribute(("name", filename.as_str()))
                                                    .write_empty()?;
                                                nv.create_element("pic:cNvPicPr").write_empty()?;
                                                Ok(())
                                            })?;
                                            pic.create_element("pic:blipFill").write_inner_content(|bf| {
                                                bf.create_element("a:blip")
                                                    .with_attribute(("r:embed", rid))
                                                    .write_empty()?;
                                                bf.create_element("a:stretch").write_inner_content(|st| {
                                                    st.create_element("a:fillRect").write_empty()?;
                                                    Ok(())
                                                })?;
                                                Ok(())
                                            })?;
                                            pic.create_element("pic:spPr").write_inner_content(|sp| {
                                                sp.create_element("a:xfrm").write_inner_content(|xf| {
                                                    xf.create_element("a:off")
                                                        .with_attribute(("x", "0"))
                                                        .with_attribute(("y", "0"))
                                                        .write_empty()?;
                                                    xf.create_element("a:ext")
                                                        .with_attribute(("cx", cx.as_str()))
                                                        .with_attribute(("cy", cy.as_str()))
                                                        .write_empty()?;
                                                    Ok(())
                                                })?;
                                                sp.create_element("a:prstGeom")
                                                    .with_attribute(("prst", "rect"))
                                                    .write_inner_content(|pg| {
                                                        pg.create_element("a:avLst").write_empty()?;
                                                        Ok(())
                                                    })?;
                                                Ok(())
                                            })?;
                                            Ok(())
                                        })?;
                                    Ok(())
                                })?;
                            Ok(())
                        })?;
                    Ok(())
                })?;
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}
