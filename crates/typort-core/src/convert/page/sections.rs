use typort_ooxml::document::{PageSettings, SectionBreak, SectionBreakType};
use typst_layout::PagedDocument;

use super::margin::default_margin_pt;
use super::units::pt_to_twips;

/// Page-dimension difference in twips that signals a section change.
const SECTION_DIMENSION_TOLERANCE_TWIPS: u32 = 20;

/// Represents a section detected from page setting changes.
#[derive(Debug)]
pub struct DetectedSection {
    /// 0-based index of the page where this section STARTS (the new section's first page).
    pub start_page: usize,
    /// Page settings for this section.
    pub page_settings: PageSettings,
}

/// Detect section breaks from page setting changes in the `PagedDocument`.
///
/// Returns a list of sections where page settings change. The first element
/// (if any) represents a change starting at `start_page` index. Each section's
/// `page_settings` describes the settings BEFORE the break (i.e., the settings
/// of the section that is ending).
#[must_use]
pub fn detect_section_breaks(paged: &PagedDocument) -> Vec<DetectedSection> {
    if paged.pages().len() < 2 {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let prev_w = paged.pages()[0].frame.width();
    let prev_h = paged.pages()[0].frame.height();
    let mut prev_width = pt_to_twips(prev_w.to_pt());
    let mut prev_height = pt_to_twips(prev_h.to_pt());

    for i in 1..paged.pages().len() {
        let curr_w = pt_to_twips(paged.pages()[i].frame.width().to_pt());
        let curr_h = pt_to_twips(paged.pages()[i].frame.height().to_pt());
        if curr_w.abs_diff(prev_width) > SECTION_DIMENSION_TOLERANCE_TWIPS
            || curr_h.abs_diff(prev_height) > SECTION_DIMENSION_TOLERANCE_TWIPS
        {
            let margin = default_margin_pt(
                prev_w.to_pt().min(paged.pages()[i].frame.width().to_pt()),
                prev_h.to_pt().min(paged.pages()[i].frame.height().to_pt()),
            );
            let margin_twips = pt_to_twips(margin);
            sections.push(DetectedSection {
                start_page: i,
                page_settings: PageSettings {
                    width_twips: prev_width,
                    height_twips: prev_height,
                    margin_top: margin_twips,
                    margin_bottom: margin_twips,
                    margin_left: margin_twips,
                    margin_right: margin_twips,
                    columns: None,
                    column_spacing: None,
                },
            });
        }
        prev_width = curr_w;
        prev_height = curr_h;
    }

    sections
}

/// Apply section breaks to the document model.
///
/// Given detected section boundaries from `PagedDocument` and an
/// element → page mapping, place `SectionBreak` on the last paragraph
/// before each section boundary.
///
/// `element_page_map` maps each element index to its 1-based page number.
/// If the map is empty, falls back to proportional mapping.
pub fn apply_section_breaks(
    doc: &mut typort_ooxml::document::Document,
    sections: &[DetectedSection],
    element_page_map: &[usize],
) {
    if sections.is_empty() {
        return;
    }

    let total_elements = doc.body.elements.len();
    if total_elements == 0 {
        return;
    }

    for section in sections {
        // Find the last element on the page before the section break.
        // section.start_page is the 0-based index of the new section's
        // first page, so we look for the last element on page `start_page`
        // (1-based) — i.e., the element just before the new section begins.
        let target_page = section.start_page; // 0-based → 1-based = start_page itself

        let approx_idx = if !element_page_map.is_empty() && element_page_map.len() == total_elements
        {
            // Use the introspector-based mapping: find the last element
            // whose page number is < start_page+1 (i.e., on a page before
            // the new section).
            element_page_map
                .iter()
                .enumerate()
                .rev()
                .find(|(_, page)| **page <= target_page)
                .map_or(0, |(idx, _)| idx)
        } else {
            // Fallback: proportional mapping (legacy behaviour)
            let total_pages = element_page_map
                .len()
                .max(sections.last().map_or(1, |s| s.start_page + 1));
            super::stats::proportional_index(section.start_page, total_pages, total_elements)
        };

        // Find the nearest paragraph at or before `approx_idx`
        let para_idx = find_nearest_paragraph(&doc.body.elements, approx_idx);
        if let Some(idx) = para_idx
            && let typort_ooxml::document::BlockElement::Paragraph(para) =
                &mut doc.body.elements[idx]
        {
            para.section_break = Some(SectionBreak {
                break_type: SectionBreakType::NextPage,
                page_settings: Some(section.page_settings.clone()),
            });
        }
    }
}

/// Find the nearest paragraph at or before the given index.
fn find_nearest_paragraph(
    elements: &[typort_ooxml::document::BlockElement],
    target: usize,
) -> Option<usize> {
    let target = target.min(elements.len().saturating_sub(1));
    // Search backwards from target for a paragraph
    for i in (0..=target).rev() {
        if matches!(
            elements[i],
            typort_ooxml::document::BlockElement::Paragraph(_)
        ) {
            return Some(i);
        }
    }
    None
}
