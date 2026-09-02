use typort_ooxml::document::{
    Alignment, HeaderFooter, PageNumberFormat, PageSettings, Paragraph, Run,
};
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

use super::units::pt_to_twips;

/// Fraction of each margin retained as a guard between body and margin content.
const BODY_ZONE_MARGIN_RATIO: f64 = 0.9;
/// Fraction of page center used to recognize centered margin text.
const MARGIN_CENTER_TOLERANCE_RATIO: f64 = 0.15;

/// A text fragment extracted from a page frame, with position info.
pub(super) struct TextFragment {
    pub(super) y: f64,
    pub(super) x: f64,
    pub(super) text: String,
}

/// Collect text fragments with absolute positions from a frame.
pub(super) fn collect_text_fragments(frame: &Frame, items: &mut Vec<TextFragment>) {
    super::super::frames::visit_frame_items(frame, false, &mut |position, item| {
        if let FrameItem::Text(text_item) = item {
            let text = text_item.text.to_string();
            if !text.is_empty() {
                items.push(TextFragment {
                    y: position.y.to_pt(),
                    x: position.x.to_pt(),
                    text,
                });
            }
        }
    });
}

/// Compute the default Typst margin for a page in pt.
///
/// Typst uses `2.5/21 * min(width, height)` as the default margin.
pub(super) fn default_margin_pt(page_width: f64, page_height: f64) -> f64 {
    let smaller = page_width.min(page_height);
    2.5 / 21.0 * smaller
}

/// Extract page dimensions from the `PagedDocument` and apply to `PageSettings`.
pub fn extract_page_settings(paged: &PagedDocument, settings: &mut PageSettings) {
    let Some(page) = paged.pages().first() else {
        return;
    };
    let w = page.frame.width().to_pt();
    let h = page.frame.height().to_pt();
    settings.width_twips = pt_to_twips(w);
    settings.height_twips = pt_to_twips(h);

    let default_margin = default_margin_pt(w, h);
    let default_twips = pt_to_twips(default_margin);
    settings.margin_top = default_twips;
    settings.margin_bottom = default_twips;
    settings.margin_left = default_twips;
    settings.margin_right = default_twips;
}

/// Identify the body content zone using margin-based boundaries.
///
/// Resolved top/bottom page margins in pt — the same values the docx writes to
/// `w:pgMar` (paged default overridden by `#set page(margin:)` from the source
/// AST). Body-zone consumers take this so their header/footer boundary tracks
/// the document's actual margins: with author margins smaller than Typst's
/// default, using the default boundary silently drops real body content.
#[derive(Clone, Copy)]
pub struct MarginsPt {
    pub top: f64,
    pub bottom: f64,
}

impl MarginsPt {
    /// Read the resolved margins from the document's `PageSettings`.
    #[must_use]
    pub fn from_settings(settings: &PageSettings) -> Self {
        Self {
            top: f64::from(settings.margin_top) / 20.0,
            bottom: f64::from(settings.margin_bottom) / 20.0,
        }
    }
}

/// Typst renders headers in the top margin area and footers in the
/// bottom margin area. We use the actual margins as the boundary
/// to separate these zones.
///
/// `margin_top_pt` and `margin_bottom_pt`: actual margins in pt. If `None`,
/// falls back to Typst's default margin (`2.5/21 * min(w, h)`).
///
/// Returns `(body_top, body_bottom)` in pt — the y-range of the body zone.
pub(in crate::convert) fn find_body_zone(
    page_width: f64,
    page_height: f64,
    margin_top_pt: Option<f64>,
    margin_bottom_pt: Option<f64>,
) -> (f64, f64) {
    let default_margin = default_margin_pt(page_width, page_height);
    let mt = margin_top_pt.unwrap_or(default_margin);
    let mb = margin_bottom_pt.unwrap_or(default_margin);
    // Body starts at the top margin line and ends at the bottom margin line.
    // Headers are positioned at margin * (1 - header_ascent) where header_ascent
    // defaults to 0.3, so header text is at ~margin * 0.7 from top.
    // Use margin * 0.9 as the boundary to safely include all header content
    // above the body zone.
    let body_top = mt * BODY_ZONE_MARGIN_RATIO;
    let body_bottom = page_height - mb * BODY_ZONE_MARGIN_RATIO;
    (body_top, body_bottom)
}

/// Extract header content from the top margin area of the first page.
#[must_use]
pub fn extract_header(paged: &PagedDocument, margins: MarginsPt) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Top, margins)
}

/// Extract footer content from the bottom margin area of the first page.
#[must_use]
pub fn extract_footer(paged: &PagedDocument, margins: MarginsPt) -> Option<HeaderFooter> {
    extract_margin_zone(paged, MarginZone::Bottom, margins)
}

#[derive(Clone, Copy)]
enum MarginZone {
    Top,
    Bottom,
}

fn extract_margin_zone(
    paged: &PagedDocument,
    zone: MarginZone,
    margins: MarginsPt,
) -> Option<HeaderFooter> {
    let page = paged.pages().first()?;
    let page_width = page.frame.width().to_pt();
    let page_height = page.frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(&page.frame, &mut fragments);

    let (body_top, body_bottom) = find_body_zone(
        page_width,
        page_height,
        Some(margins.top),
        Some(margins.bottom),
    );

    let mut items: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| match zone {
            MarginZone::Top => f.y < body_top && f.y > 0.0,
            MarginZone::Bottom => f.y > body_bottom && f.y <= page_height,
        })
        .collect();

    if items.is_empty() {
        return None;
    }

    items.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    let mut para = Paragraph::new();
    for item in &items {
        para.push_run(Run::new(&item.text));
    }

    // Detect alignment — use 15% of page center as threshold to avoid
    // false-positives on left-aligned text that starts near the center.
    let page_center = page_width / 2.0;
    if items.iter().all(|f| f.x > page_center) {
        para.alignment = Some(Alignment::Right);
    } else if items
        .iter()
        .all(|f| (f.x - page_center).abs() < page_center * MARGIN_CENTER_TOLERANCE_RATIO)
    {
        para.alignment = Some(Alignment::Center);
    }

    Some(HeaderFooter {
        paragraphs: vec![para],
    })
}

// ---------------------------------------------------------------------------
// Page number detection
// ---------------------------------------------------------------------------

/// Check if the footer text on the first page looks like a page number.
///
/// Returns `Some(PageNumberFormat)` if the footer is just a page number
/// (e.g. "1", "i", "I", "a", "A"), otherwise `None`.
///
/// We check multiple pages to confirm — if different pages have different
/// consecutive numbers, it's definitely a page number rather than static text.
/// A single footer "i" (a word) or "5" (a static label) would be misclassified
/// without multi-page verification.
#[must_use]
pub fn detect_page_numbering(
    paged: &PagedDocument,
    margins: MarginsPt,
) -> Option<PageNumberFormat> {
    if paged.pages().is_empty() {
        return None;
    }

    // Extract footer text from the first page
    let first_footer = extract_footer_text_from_page(&paged.pages()[0].frame, margins)?;
    let first_trimmed = first_footer.trim();

    // Try to classify the text as a page number format
    let fmt = classify_page_number(first_trimmed)?;

    // If we have a second page, verify consecutiveness to avoid false positives
    if paged.pages().len() >= 2 {
        let second_footer = extract_footer_text_from_page(&paged.pages()[1].frame, margins);
        // Second page has no footer text — can't confirm page numbering
        let text = second_footer?;
        let second_trimmed = text.trim();
        let fmt2 = classify_page_number(second_trimmed);
        // Both pages must have the same format
        if fmt2.as_ref() != Some(&fmt) {
            return None;
        }
        // Values must be consecutive (page 2 value = page 1 value + 1)
        let val1 = page_number_value(first_trimmed, &fmt);
        let val2 = page_number_value(second_trimmed, &fmt);
        if val1 == 0 || val2 == 0 || val2 != val1 + 1 {
            return None;
        }
    } else {
        // Single-page document: only accept a single-digit number "1" as
        // reasonably likely to be page numbering. Other formats ("i", "a",
        // etc.) are too ambiguous without a second page to confirm.
        if first_trimmed != "1" {
            return None;
        }
    }

    Some(fmt)
}

/// Get the numeric value of a page number string for a given format.
///
/// Returns 0 if the string cannot be parsed for the given format.
fn page_number_value(s: &str, fmt: &PageNumberFormat) -> u32 {
    match fmt {
        PageNumberFormat::Decimal => s.parse::<u32>().unwrap_or(0),
        PageNumberFormat::LowerRoman => roman_value(s, false),
        PageNumberFormat::UpperRoman => roman_value(s, true),
        PageNumberFormat::LowerLetter => {
            if s.len() == 1 {
                let c = s.chars().next().unwrap();
                if c.is_ascii_lowercase() {
                    return u32::from(c) - u32::from('a') + 1;
                }
            }
            0
        }
        PageNumberFormat::UpperLetter => {
            if s.len() == 1 {
                let c = s.chars().next().unwrap();
                if c.is_ascii_uppercase() {
                    return u32::from(c) - u32::from('A') + 1;
                }
            }
            0
        }
    }
}

/// Extract footer text from a single page frame (text in the bottom margin zone).
fn extract_footer_text_from_page(frame: &Frame, margins: MarginsPt) -> Option<String> {
    let page_width = frame.width().to_pt();
    let page_height = frame.height().to_pt();

    let mut fragments = Vec::new();
    collect_text_fragments(frame, &mut fragments);

    let (_body_top, body_bottom) = find_body_zone(
        page_width,
        page_height,
        Some(margins.top),
        Some(margins.bottom),
    );

    let footer_items: Vec<&TextFragment> = fragments
        .iter()
        .filter(|f| f.y > body_bottom && f.y <= page_height)
        .collect();

    if footer_items.is_empty() {
        return None;
    }

    let text: String = footer_items.iter().map(|f| f.text.as_str()).collect();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Classify a string as a page number format.
///
/// - "1", "2", ... → `Decimal`
/// - "i", "ii", "iii", "iv", ... → `LowerRoman`
/// - "I", "II", "III", "IV", ... → `UpperRoman`
/// - "a", "b", "c", ... → `LowerLetter`
/// - "A", "B", "C", ... → `UpperLetter`
fn classify_page_number(s: &str) -> Option<PageNumberFormat> {
    if s.is_empty() {
        return None;
    }

    // Decimal: pure digits
    if s.chars().all(|c| c.is_ascii_digit()) {
        return Some(PageNumberFormat::Decimal);
    }

    // Roman numerals (lowercase): i, ii, iii, iv, v, vi, vii, viii, ix, x, ...
    if is_lower_roman(s) {
        return Some(PageNumberFormat::LowerRoman);
    }

    // Roman numerals (uppercase): I, II, III, IV, V, ...
    if is_upper_roman(s) {
        return Some(PageNumberFormat::UpperRoman);
    }

    // Single letter: a-z or A-Z (page 1 = a, page 2 = b, etc.)
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        if c.is_ascii_lowercase() {
            return Some(PageNumberFormat::LowerLetter);
        }
        if c.is_ascii_uppercase() {
            return Some(PageNumberFormat::UpperLetter);
        }
    }

    None
}

/// Check if a string is a valid lowercase Roman numeral.
fn is_lower_roman(s: &str) -> bool {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'))
    {
        return false;
    }
    // Validate by converting and checking it's a reasonable page number
    roman_value(s, false) > 0
}

/// Check if a string is a valid uppercase Roman numeral.
fn is_upper_roman(s: &str) -> bool {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
    {
        return false;
    }
    roman_value(s, true) > 0
}

/// Compute the numeric value of a Roman numeral string.
fn roman_value(s: &str, uppercase: bool) -> u32 {
    let val = |c: char| -> u32 {
        match if uppercase { c.to_ascii_lowercase() } else { c } {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => 0,
        }
    };

    let chars: Vec<char> = s.chars().collect();
    let mut total: u32 = 0;
    for i in 0..chars.len() {
        let curr = val(chars[i]);
        let next = if i + 1 < chars.len() {
            val(chars[i + 1])
        } else {
            0
        };
        if curr < next {
            total = total.wrapping_sub(curr);
        } else {
            total = total.wrapping_add(curr);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_decimal() {
        assert_eq!(classify_page_number("1"), Some(PageNumberFormat::Decimal));
        assert_eq!(classify_page_number("42"), Some(PageNumberFormat::Decimal));
        assert_eq!(classify_page_number("100"), Some(PageNumberFormat::Decimal));
    }

    #[test]
    fn classify_lower_roman() {
        assert_eq!(
            classify_page_number("i"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("ii"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("iv"),
            Some(PageNumberFormat::LowerRoman)
        );
        assert_eq!(
            classify_page_number("xiv"),
            Some(PageNumberFormat::LowerRoman)
        );
    }

    #[test]
    fn classify_upper_roman() {
        assert_eq!(
            classify_page_number("I"),
            Some(PageNumberFormat::UpperRoman)
        );
        assert_eq!(
            classify_page_number("IV"),
            Some(PageNumberFormat::UpperRoman)
        );
        assert_eq!(
            classify_page_number("XII"),
            Some(PageNumberFormat::UpperRoman)
        );
    }

    #[test]
    fn classify_letters() {
        assert_eq!(
            classify_page_number("a"),
            Some(PageNumberFormat::LowerLetter)
        );
        assert_eq!(
            classify_page_number("z"),
            Some(PageNumberFormat::LowerLetter)
        );
        assert_eq!(
            classify_page_number("A"),
            Some(PageNumberFormat::UpperLetter)
        );
        assert_eq!(
            classify_page_number("Z"),
            Some(PageNumberFormat::UpperLetter)
        );
    }

    #[test]
    fn classify_non_page_numbers() {
        assert_eq!(classify_page_number(""), None);
        assert_eq!(classify_page_number("Draft"), None);
        assert_eq!(classify_page_number("Page 1"), None);
        assert_eq!(classify_page_number("hello"), None);
    }

    #[test]
    fn page_number_value_decimal() {
        assert_eq!(page_number_value("1", &PageNumberFormat::Decimal), 1);
        assert_eq!(page_number_value("5", &PageNumberFormat::Decimal), 5);
        assert_eq!(page_number_value("42", &PageNumberFormat::Decimal), 42);
    }

    #[test]
    fn page_number_value_roman() {
        assert_eq!(page_number_value("i", &PageNumberFormat::LowerRoman), 1);
        assert_eq!(page_number_value("ii", &PageNumberFormat::LowerRoman), 2);
        assert_eq!(page_number_value("iii", &PageNumberFormat::LowerRoman), 3);
        assert_eq!(page_number_value("iv", &PageNumberFormat::LowerRoman), 4);
        assert_eq!(page_number_value("v", &PageNumberFormat::LowerRoman), 5);
        assert_eq!(page_number_value("IX", &PageNumberFormat::UpperRoman), 9);
        assert_eq!(page_number_value("X", &PageNumberFormat::UpperRoman), 10);
    }

    #[test]
    fn page_number_value_letters() {
        assert_eq!(page_number_value("a", &PageNumberFormat::LowerLetter), 1);
        assert_eq!(page_number_value("b", &PageNumberFormat::LowerLetter), 2);
        assert_eq!(page_number_value("z", &PageNumberFormat::LowerLetter), 26);
        assert_eq!(page_number_value("A", &PageNumberFormat::UpperLetter), 1);
        assert_eq!(page_number_value("C", &PageNumberFormat::UpperLetter), 3);
    }

    #[test]
    fn consecutive_check_logic() {
        // Decimal: 1 -> 2 is consecutive
        let v1 = page_number_value("1", &PageNumberFormat::Decimal);
        let v2 = page_number_value("2", &PageNumberFormat::Decimal);
        assert_eq!(v2, v1 + 1);

        // Decimal: 5 -> 5 is NOT consecutive (static text)
        let v1 = page_number_value("5", &PageNumberFormat::Decimal);
        let v2 = page_number_value("5", &PageNumberFormat::Decimal);
        assert_ne!(v2, v1 + 1);

        // Roman: i -> ii is consecutive
        let v1 = page_number_value("i", &PageNumberFormat::LowerRoman);
        let v2 = page_number_value("ii", &PageNumberFormat::LowerRoman);
        assert_eq!(v2, v1 + 1);

        // Letter: a -> b is consecutive
        let v1 = page_number_value("a", &PageNumberFormat::LowerLetter);
        let v2 = page_number_value("b", &PageNumberFormat::LowerLetter);
        assert_eq!(v2, v1 + 1);
    }
}
