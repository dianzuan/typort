//! Text normalisation shared by the HTML walk and paged recovery.

use typort_ooxml::document::{InlineElement, Paragraph};

use super::page;

pub(super) fn strip_cjk_spaces(para: &mut Paragraph) {
    let mut remove_indices = Vec::new();
    for i in 1..para.inlines.len().saturating_sub(1) {
        let InlineElement::Text(run) = &para.inlines[i] else {
            continue;
        };
        if run.text.trim() != "" {
            continue;
        }
        let prev = &para.inlines[i - 1];
        let next = &para.inlines[i + 1];
        let prev_ends_cjk = matches!(prev, InlineElement::Text(r)
            if r.text.chars().last().is_some_and(page::is_cjk_char));
        let next_starts_cjk = matches!(next, InlineElement::Text(r)
            if r.text.chars().next().is_some_and(page::is_cjk_char));
        let prev_is_math = matches!(prev, InlineElement::Math { .. });
        let next_is_math = matches!(next, InlineElement::Math { .. });
        // A space adjacent to CJK on one side carries no meaning when the other
        // side is CJK text or an inline equation — Chinese needs no separator from
        // a neighbouring character or formula. (A space between Latin text and an
        // equation IS kept: Typst trims the source space and Word needs it back,
        // e.g. "the value x is".)
        if (prev_ends_cjk && (next_starts_cjk || next_is_math)) || (prev_is_math && next_starts_cjk)
        {
            remove_indices.push(i);
        }
    }
    for idx in remove_indices.into_iter().rev() {
        para.inlines.remove(idx);
    }
}

pub(super) fn strip_visual_markers(s: &str) -> String {
    let trimmed = s.trim_start_matches(['•', '‣', '◦', '▪', '▸', '–', '—']);
    let trimmed = trimmed.trim_start();
    // Strip leading "1." or "1.1" or "1.1.1" numbering patterns
    let trimmed = if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        rest.trim_start()
    } else {
        trimmed
    };
    trimmed.to_string()
}

pub(super) fn strip_cjk_spaces_str(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' '
            && i > 0
            && i + 1 < chars.len()
            && page::is_cjk_char(chars[i - 1])
            && page::is_cjk_char(chars[i + 1])
        {
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Map Unicode Mathematical Italic/Bold/Script characters to ASCII equivalents.
/// Paged output renders math as Unicode math italic (U+1D400-U+1D7FF) while
/// OMML stores them as plain ASCII with formatting attributes.
pub(super) fn strip_math_italic(text: &str) -> String {
    text.chars()
        .map(|c| {
            let cp = u32::from(c);
            match cp {
                // Math italic small a-z: U+1D44E-U+1D467
                0x1D44E..=0x1D467 => math_letter(c, cp, 0x1D44E, 'a'),
                // Math italic capital A-Z: U+1D434-U+1D44D
                0x1D434..=0x1D44D => math_letter(c, cp, 0x1D434, 'A'),
                // Math bold small a-z: U+1D41A-U+1D433
                0x1D41A..=0x1D433 => math_letter(c, cp, 0x1D41A, 'a'),
                // Math bold capital A-Z: U+1D400-U+1D419
                0x1D400..=0x1D419 => math_letter(c, cp, 0x1D400, 'A'),
                // Math bold italic small a-z: U+1D482-U+1D49B
                0x1D482..=0x1D49B => math_letter(c, cp, 0x1D482, 'a'),
                // Math bold italic capital A-Z: U+1D468-U+1D481
                0x1D468..=0x1D481 => math_letter(c, cp, 0x1D468, 'A'),
                // Math italic h: U+210E
                0x210E => 'h',
                _ => c,
            }
        })
        .collect()
}

fn math_letter(original: char, codepoint: u32, range_start: u32, ascii_start: char) -> char {
    char::from_u32(u32::from(ascii_start) + codepoint - range_start).unwrap_or(original)
}

/// Cancel every whitespace character, so a paged render and the emitted text of the
/// same heading compare equal despite the `format!("{numbers} ")` space (and any
/// layout spacing) between a heading's number and its title.
pub(super) fn cancel_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether `c` is a CJK ideograph (the ranges used for projection/fragments).
pub(super) fn is_cjk_ideograph(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}

/// Remove inline citation/footnote markers like `[12]`, `[1,2]` or `[1-3]` from a
/// line. Such marks are already emitted as citations/footnote refs, so a paged
/// line is not "missing" merely because it carries (or is made entirely of) them.
pub(super) fn strip_citation_markers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(rel) = chars[i + 1..].iter().position(|c| *c == ']')
        {
            let inner = &chars[i + 1..i + 1 + rel];
            if !inner.is_empty()
                && inner
                    .iter()
                    .all(|c| c.is_ascii_digit() || matches!(c, ',' | '，' | ' ' | '-' | '–'))
            {
                i = i + 1 + rel + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Extract contiguous CJK ideograph runs of at least `min_len` characters.
/// Uses only CJK Unified Ideographs (not fullwidth punctuation) so that
/// fragments like "被主流接受" match regardless of surrounding punctuation.
pub(super) fn extract_cjk_fragments(text: &str, min_len: usize) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}') {
            current.push(c);
        } else if current.chars().count() >= min_len {
            fragments.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.chars().count() >= min_len {
        fragments.push(current);
    }
    fragments
}
