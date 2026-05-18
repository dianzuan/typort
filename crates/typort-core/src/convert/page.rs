//! `PagedDocument` -> page settings, document style, font detection.
//!
//! Extracts the most-common font family / size from rendered frames
//! and computes page dimensions + margins from the first page.

use std::collections::HashMap;

use typort_ooxml::document::{DocumentStyle, FootnoteFormat, PageSettings};
use typst::layout::{Frame, FrameItem, PagedDocument, Point};

/// Extract document style (fonts, sizes, spacing) from the rendered `PagedDocument`.
///
/// Walks the first few pages' frames to find the most common font family and size,
/// which represent the body text styling.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn extract_document_style(paged: &PagedDocument) -> DocumentStyle {
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
        .map_or(21, |(size, _)| size); // default 10.5pt

    let body_font_ascii = body_font.clone();
    let body_font_east_asia = body_font;

    // Compute first-line indent: 2 chars at body size (standard CJK convention)
    let body_pt = f64::from(body_size_half_pt) / 2.0;
    let first_line_indent_twips = (body_pt * 20.0 * 2.0).round() as u32;

    let line_spacing = 360; // 1.5x (360/240 of a line)

    DocumentStyle {
        body_font_ascii,
        body_font_east_asia,
        body_size_half_pt,
        line_spacing,
        first_line_indent_twips,
        footnote_format: FootnoteFormat::default(),
    }
}

/// Recursively collect font family and size information from frame items.
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

/// Extract page dimensions from the `PagedDocument` and apply to `PageSettings`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn extract_page_settings(paged: &PagedDocument, settings: &mut PageSettings) {
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

/// Recursively collect content bounding box from frame items.
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
