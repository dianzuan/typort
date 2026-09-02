use std::collections::{HashMap, HashSet};

use typort_ooxml::document::{Document, Run};
use typst::World;

pub(crate) fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' |
        '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}' |
        '\u{AC00}'..='\u{D7AF}' | '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}' |
        '\u{FE30}'..='\u{FE4F}' |
        '\u{2E80}'..='\u{2EFF}' | '\u{2F00}'..='\u{2FDF}' |
        '\u{20000}'..='\u{2A6DF}'
    )
}

/// Build a BCP-47 language tag (for Word's `w:lang`) from a Typst `lang` code
/// plus an optional `region`, exactly as declared in `#set text(...)`.
///
/// `lang` is an ISO 639 code ("zh", "ja", "de"); `region` is ISO 3166-1
/// ("CN", "JP"). When the region is omitted, fall back to the most common
/// region for that language so Word still gets a fully-qualified tag (Word's
/// East-Asian handling expects one); unknown languages pass through bare.
pub(in crate::convert) fn lang_region_to_bcp47(lang: &str, region: Option<&str>) -> String {
    let lang = lang.to_ascii_lowercase();
    if let Some(r) = region {
        return format!("{lang}-{}", r.to_ascii_uppercase());
    }
    match lang.as_str() {
        "zh" => "zh-CN".to_string(),
        "ja" => "ja-JP".to_string(),
        "ko" => "ko-KR".to_string(),
        "en" => "en-US".to_string(),
        "de" => "de-DE".to_string(),
        "fr" => "fr-FR".to_string(),
        _ => lang,
    }
}

/// Windows language IDs (LCIDs) for the localized font-name records that match a
/// document's declared East-Asian language tag, in preference order. Returns an
/// empty list for non-CJK tags (no localized name to prefer over the typographic
/// English family). Drives [`localize_cjk_font_names`].
fn cjk_localized_lang_ids(lang_tag: &str) -> Vec<u16> {
    let tag = lang_tag.to_ascii_lowercase();
    let mut subtags = tag.split(['-', '_']);
    let primary = subtags.next().unwrap_or("");
    let rest: Vec<&str> = subtags.collect();
    match primary {
        "zh" => {
            // Traditional-script regions/script subtag prefer the Taiwan/HK/Macau
            // name records; everything else (CN, SG, Hans, or a bare `zh`) is
            // Simplified and prefers the PRC record. The English typographic
            // family stays the fallback if neither localized record exists.
            let traditional = rest
                .iter()
                .any(|p| matches!(*p, "tw" | "hk" | "mo" | "hant"));
            if traditional {
                vec![0x0404, 0x0C04, 0x1404, 0x0804]
            } else {
                vec![0x0804, 0x1004, 0x0404]
            }
        }
        "ja" => vec![0x0411],
        "ko" => vec![0x0412],
        _ => Vec::new(),
    }
}

/// Rewrite every East-Asian font name in `doc` to its localized form using
/// `name_map` (rendered English family → localized display name). Typst exposes
/// these CJK fonts only by their English typographic name (e.g. `SimSun`), so the
/// `w:eastAsia` strings that reach Word are English; Word/users expect the
/// localized name (`宋体`). Latin (`w:ascii`) fonts and families absent from the
/// map are left untouched. A no-op when `name_map` is empty.
fn localize_cjk_font_names(doc: &mut Document, name_map: &HashMap<String, String>) {
    if name_map.is_empty() {
        return;
    }
    if let Some(localized) = name_map.get(&doc.style.body_font_east_asia) {
        doc.style.body_font_east_asia = localized.clone();
    }
    doc.for_each_run_mut(&mut |run| localize_run_font(run, name_map));
}

/// Remap a single run's East-Asian font via `name_map`. Latin (`font_ascii`) is
/// never touched — only `w:eastAsia` carries the localizable CJK family.
fn localize_run_font(run: &mut Run, name_map: &HashMap<String, String>) {
    if let Some(family) = &run.font_east_asia
        && let Some(localized) = name_map.get(family)
    {
        run.font_east_asia = Some(localized.clone());
    }
}

/// The localized FAMILY (name ID 1) string from a font's name table for the first
/// matching Windows language id in `lang_ids`, or `None` if the face carries no
/// such localized record (then the English typographic family is kept).
fn localized_family_from_face(face: &ttf_parser::Face, lang_ids: &[u16]) -> Option<String> {
    for &want in lang_ids {
        for entry in face.names() {
            if entry.name_id == ttf_parser::name_id::FAMILY
                && entry.platform_id == ttf_parser::PlatformId::Windows
                && entry.language_id == want
                && let Some(name) = entry.to_string()
            {
                return Some(name);
            }
        }
    }
    None
}

/// Translate every East-Asian font name in `doc` to the localized display name
/// Word shows for the document's declared language (e.g. `SimSun` → `宋体`).
///
/// Typst indexes these CJK fonts only by their English typographic family
/// (`info.family`), so both the source-declared body default *and* the rendered
/// per-run families that reach `w:eastAsia` are English. The localized name lives
/// in each font's own `name` table; we read it from the **font book** (not the
/// rendered frames) so it works even when Typst substitutes a different face at
/// render time — e.g. Typst's known `SimSun`→fallback substitution (typst#6205),
/// where the body previews in Noto Serif SC yet the author clearly meant `SimSun`.
///
/// A no-op for non-CJK documents and for fonts that carry no differing localized
/// name (then the English family is kept). Pure-string translation only — it
/// never changes which glyphs Word renders, just the font *name* in the box.
pub fn localize_cjk_fonts(world: &dyn World, doc: &mut Document) {
    let lang_ids = cjk_localized_lang_ids(&doc.style.lang_east_asia);
    if lang_ids.is_empty() {
        return;
    }
    let mut name_map: HashMap<String, String> = HashMap::new();
    for family in collect_cjk_font_names(doc) {
        if let Some(localized) = localized_name_via_book(world, &family, &lang_ids)
            && localized != family
        {
            name_map.insert(family, localized);
        }
    }
    localize_cjk_font_names(doc, &name_map);
}

/// The distinct East-Asian font family names currently in `doc`: the body default
/// plus every per-run `font_east_asia` override (body, tables, bibliography,
/// hyperlinks, footnotes).
fn collect_cjk_font_names(doc: &Document) -> HashSet<String> {
    let mut names = HashSet::new();
    names.insert(doc.style.body_font_east_asia.clone());
    doc.for_each_run(&mut |run| {
        if let Some(f) = &run.font_east_asia {
            names.insert(f.clone());
        }
    });
    names
}

/// Look up `family` in the font book and read its localized FAMILY name for the
/// first matching `lang_ids` entry. Works for any installed font regardless of
/// whether it was selected during rendering.
fn localized_name_via_book(world: &dyn World, family: &str, lang_ids: &[u16]) -> Option<String> {
    // The book keys families by their lowercased English typographic name, and
    // Typst's suffix-trimming can group distinct files under one key (e.g.
    // `SimSun-ExtB` lands under `SimSun` — and it carries no localized record).
    // So scan every variant and take the first that advertises a localized name,
    // rather than the arbitrary first variant.
    //
    // But weight variants of one design can also share a key while carrying
    // DISTINCT English families: "Noto Serif SC Light" groups under "Noto Serif
    // SC", and its localized record would mislabel the requested family with the
    // weight name. So prefer a variant whose English typographic family (name ID
    // 1, LCID 0x0409) equals the requested family, and skip variants whose
    // English family differs from it.
    world
        .book()
        .select_family(&family.to_lowercase())
        .filter_map(|index| world.font(index))
        .find_map(|font| {
            // typst 0.15 removed `Font::ttf`; the `ttf_parser::Face` now lives on
            // `FontInstance`. Re-parse the face from the font's own buffer and
            // collection index (exactly what `Font::new` does internally) so we
            // can still read its name table for localized CJK family names.
            let face = ttf_parser::Face::parse(font.data(), font.index()).ok()?;
            // Skip variants whose English typographic family differs from the
            // requested one (e.g. a same-keyed "Noto Serif SC Light"). The
            // English family is the same FAMILY lookup with the en-US LCID.
            if localized_family_from_face(&face, &[0x0409])
                .is_some_and(|eng| !eng.eq_ignore_ascii_case(family))
            {
                return None;
            }
            localized_family_from_face(&face, lang_ids)
        })
}

/// Build per-span and per-text style override maps from paged run styles.
///
/// Compares each rendered run against the detected baselines (font + size)
/// and emits an override entry only when the run differs from the baseline.
#[cfg(test)]
mod tests {
    use super::*;
    use typort_ooxml::document::Paragraph;
    #[test]
    fn cjk_localized_lang_ids_zh_cn_prefers_prc() {
        // zh-CN must prefer the PRC localized-name record (LCID 0x0804 → 宋体).
        assert_eq!(
            cjk_localized_lang_ids("zh-CN").first().copied(),
            Some(0x0804)
        );
    }

    #[test]
    fn cjk_localized_lang_ids_non_cjk_is_empty() {
        // A Latin document has no localized CJK name to prefer.
        assert!(cjk_localized_lang_ids("en-US").is_empty());
    }

    #[test]
    fn localize_cjk_font_names_remaps_body_default_and_runs() {
        // At this pipeline stage a zh-CN document carries English CJK family names
        // (SimSun/SimHei). The localized-name map translates them to what Word
        // shows in its font box; Latin fonts and unmapped families stay as-is.
        let mut doc = Document::new();
        doc.style.body_font_ascii = "Times New Roman".to_string();
        doc.style.body_font_east_asia = "SimSun".to_string();

        let mut para = Paragraph::new();
        let mut heading_run = Run::new("黑体标题");
        heading_run.font_east_asia = Some("SimHei".to_string());
        para.push_run(heading_run);
        let mut latin_run = Run::new("Latin");
        latin_run.font_ascii = Some("Arial".to_string());
        para.push_run(latin_run);
        let mut unmapped_run = Run::new("仿宋");
        unmapped_run.font_east_asia = Some("FangSong".to_string());
        para.push_run(unmapped_run);
        doc.body
            .elements
            .push(typort_ooxml::document::BlockElement::Paragraph(para));

        let name_map = HashMap::from([
            ("SimSun".to_string(), "宋体".to_string()),
            ("SimHei".to_string(), "黑体".to_string()),
        ]);
        localize_cjk_font_names(&mut doc, &name_map);

        assert_eq!(
            doc.style.body_font_east_asia, "宋体",
            "body default SimSun → 宋体"
        );
        let runs: Vec<&Run> = doc
            .body
            .elements
            .iter()
            .flat_map(|e| {
                if let typort_ooxml::document::BlockElement::Paragraph(p) = e {
                    p.text_runs().collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            })
            .collect();
        assert_eq!(
            runs[0].font_east_asia.as_deref(),
            Some("黑体"),
            "per-run SimHei → 黑体"
        );
        assert_eq!(
            runs[1].font_ascii.as_deref(),
            Some("Arial"),
            "Latin ascii font is left untouched"
        );
        assert_eq!(
            runs[2].font_east_asia.as_deref(),
            Some("FangSong"),
            "a family absent from the map is left untouched"
        );
    }
}
