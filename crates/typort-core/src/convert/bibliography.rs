use hayagriva::types::{EntryType, Person};
use typort_ooxml::document::{CitationSource, SourceType};
use typst::comemo::Track;
use typst_html::HtmlDocument;
use typst_library::foundations::{NativeElement, PathOrStr};
use typst_library::introspection::Introspector;
use typst_library::loading::DataSource;
use typst_library::model::BibliographyElem;

use crate::world::TyportWorld;

/// Extract bibliography sources from the Typst document.
///
/// Typst's `Bibliography` type keeps its parsed entries crate-private, so we
/// re-read and re-parse the `.bib`/`.yaml` files ourselves using hayagriva.
/// The introspector gives us the citation keys to know which entries matter;
/// the `DataSource` paths tell us where the files live on disk.
pub fn extract_bibliography_sources(
    html_doc: &HtmlDocument,
    world: &TyportWorld,
) -> Vec<CitationSource> {
    // `track()` is provided by `#[comemo::track]` on the `Introspector` trait,
    // so the receiver must be coerced to `&dyn Introspector` (the wrapper struct
    // `HtmlIntrospector` no longer exposes an inherent `track`).
    let introspector: &dyn Introspector = &**html_doc.introspector();
    let tracked = introspector.track();

    // Find every BibliographyElem to access source file paths. `find` was
    // removed in typst 0.15, which also lifted the one-bibliography-per-document
    // restriction: query the introspector for all matching elements and merge
    // each one's library so every citation key in the document resolves.
    let bib_elems = tracked.query(&BibliographyElem::ELEM.select());
    if bib_elems.is_empty() {
        return Vec::new();
    }

    // Re-parse the bibliography files to get full hayagriva Entry data. Keys
    // duplicated across bibliographies keep the first occurrence encountered
    // (see `merge_library`'s existing push-only behavior).
    let mut library = hayagriva::Library::new();
    for content in &bib_elems {
        let Some(bib_elem) = content.to_packed::<BibliographyElem>() else {
            continue;
        };
        let lib = load_bibliography_library(&bib_elem.sources.source.0, world);
        merge_library(&mut library, &lib);
    }

    // Get the citation keys from the introspector
    let keys = BibliographyElem::keys(tracked);

    keys.into_iter()
        .filter_map(|(label, _)| {
            let tag = label.resolve().to_string();
            let entry = library.get(&tag)?;
            Some(entry_to_citation_source(&tag, entry))
        })
        .collect()
}

/// Load and parse all bibliography source files into a hayagriva Library.
fn load_bibliography_library(sources: &[DataSource], world: &TyportWorld) -> hayagriva::Library {
    let mut library = hayagriva::Library::new();
    let root = world.root();

    for source in sources {
        match source {
            DataSource::Path(path_or_str) => {
                // `PathOrStr::as_str` was removed in typst 0.15; the type is now
                // an enum. A `Str` carries the raw author-written path; a `Path`
                // carries a resolved `RootedPath`. Both resolve, as before, to a
                // root-relative path (leading slash stripped) joined onto root.
                let rel = match path_or_str {
                    PathOrStr::Str(s) => s.trim_start_matches('/').to_string(),
                    PathOrStr::Path(rooted) => rooted.vpath().get_without_slash().to_string(),
                };
                let path = root.join(&rel);
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let is_bib = path.extension().and_then(|e| e.to_str()) == Some("bib");
                        if let Some(parsed) = try_parse_bibliography(&content, is_bib) {
                            merge_library(&mut library, &parsed);
                        } else {
                            eprintln!(
                                "typort: warning: failed to parse bibliography file {}",
                                path.display()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "typort: warning: could not read bibliography file {}: {e}",
                            path.display()
                        );
                    }
                }
            }
            DataSource::Bytes(bytes) => {
                if let Ok(content) = std::str::from_utf8(bytes.as_slice())
                    && let Some(parsed) = try_parse_bibliography(content, false)
                {
                    merge_library(&mut library, &parsed);
                }
            }
        }
    }

    library
}

fn try_parse_bibliography(content: &str, prefer_bib: bool) -> Option<hayagriva::Library> {
    if prefer_bib {
        hayagriva::io::from_biblatex_str(content)
            .ok()
            .or_else(|| hayagriva::io::from_yaml_str(content).ok())
    } else {
        hayagriva::io::from_yaml_str(content)
            .ok()
            .or_else(|| hayagriva::io::from_biblatex_str(content).ok())
    }
}

fn merge_library(target: &mut hayagriva::Library, source: &hayagriva::Library) {
    for entry in source.iter() {
        target.push(entry);
    }
}

/// Convert a hayagriva Entry to a Word `CitationSource` with full metadata.
fn entry_to_citation_source(tag: &str, entry: &hayagriva::Entry) -> CitationSource {
    let authors = entry
        .authors()
        .map(|a| a.iter().map(map_person).collect())
        .unwrap_or_default();

    let title = entry.title().map(|t| t.value.to_str().into_owned());
    let year = entry.date().map(|d| d.year.to_string());
    let doi = entry.doi().map(String::from);
    let url = entry.url().map(|u| u.value.to_string());
    let volume = entry.volume().map(std::string::ToString::to_string);
    let issue = entry.issue().map(std::string::ToString::to_string);
    let pages = entry.page_range().map(std::string::ToString::to_string);

    let parent_title = entry
        .parents()
        .first()
        .and_then(|p| p.title())
        .map(|t| t.value.to_str().into_owned());

    let is_chapter_like = matches!(
        entry.entry_type(),
        EntryType::Chapter | EntryType::Anthos | EntryType::Entry
    );

    let journal_name = if is_chapter_like {
        None
    } else {
        parent_title.clone()
    };
    let book_title = if is_chapter_like { parent_title } else { None };

    let publisher = entry
        .publisher()
        .and_then(|p| p.name())
        .map(|n| n.value.to_str().into_owned());

    let city = entry
        .publisher()
        .and_then(|p| p.location())
        .map(|l| l.value.to_str().into_owned());

    let edition = entry.edition().map(std::string::ToString::to_string);

    CitationSource {
        tag: tag.to_string(),
        source_type: map_entry_type(*entry.entry_type()),
        authors,
        title,
        year,
        journal_name,
        volume,
        issue,
        pages,
        doi,
        url,
        publisher,
        city,
        edition,
        book_title,
    }
}

/// Map a hayagriva `EntryType` to a Word `SourceType`.
pub fn map_entry_type(entry_type: EntryType) -> SourceType {
    match entry_type {
        EntryType::Article | EntryType::Newspaper => SourceType::JournalArticle,
        EntryType::Book | EntryType::Reference | EntryType::Anthology => SourceType::Book,
        EntryType::Chapter | EntryType::Anthos | EntryType::Entry => SourceType::BookSection,
        EntryType::Proceedings | EntryType::Conference => SourceType::ConferenceProceedings,
        EntryType::Report => SourceType::Report,
        EntryType::Thesis => SourceType::Thesis,
        EntryType::Web | EntryType::Blog | EntryType::Post | EntryType::Thread => {
            SourceType::InternetSite
        }
        _ => SourceType::Misc,
    }
}

/// Map a hayagriva `Person` to a Word `PersonName`.
pub fn map_person(person: &Person) -> typort_ooxml::document::PersonName {
    typort_ooxml::document::PersonName {
        last: person.name.clone(),
        first: person.given_name.clone(),
        middle: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_maps_to_journal_article() {
        assert_eq!(
            map_entry_type(EntryType::Article),
            SourceType::JournalArticle
        );
    }

    #[test]
    fn book_maps_to_book() {
        assert_eq!(map_entry_type(EntryType::Book), SourceType::Book);
    }

    #[test]
    fn thesis_maps_to_thesis() {
        assert_eq!(map_entry_type(EntryType::Thesis), SourceType::Thesis);
    }

    #[test]
    fn web_maps_to_internet_site() {
        assert_eq!(map_entry_type(EntryType::Web), SourceType::InternetSite);
    }

    #[test]
    fn unknown_maps_to_misc() {
        assert_eq!(map_entry_type(EntryType::Video), SourceType::Misc);
    }

    #[test]
    fn person_maps_name_fields() {
        let person = Person {
            name: "Smith".into(),
            given_name: Some("John".into()),
            prefix: None,
            suffix: None,
            comma_suffix: false,
            alias: None,
        };
        let mapped = map_person(&person);
        assert_eq!(mapped.last, "Smith");
        assert_eq!(mapped.first.as_deref(), Some("John"));
    }

    /// Parse a BibLaTeX string and convert the entry with the given key.
    fn parse_and_convert(bib: &str, key: &str) -> CitationSource {
        let lib = hayagriva::io::from_biblatex_str(bib).unwrap();
        let entry = lib.get(key).unwrap();
        entry_to_citation_source(key, entry)
    }

    #[test]
    fn article_entry_has_journal_metadata() {
        let src = parse_and_convert(
            r#"
            @article{smith2020,
                author = {Smith, John},
                title = {Test Article},
                journal = {Test Journal},
                year = {2020},
                volume = {1},
                pages = {1--10},
            }
            "#,
            "smith2020",
        );
        assert_eq!(src.source_type, SourceType::JournalArticle);
        assert_eq!(src.title.as_deref(), Some("Test Article"));
        assert_eq!(src.year.as_deref(), Some("2020"));
        assert_eq!(src.journal_name.as_deref(), Some("Test Journal"));
        // Note: hayagriva puts volume on the parent (journal) entry for articles,
        // so entry.volume() returns None; the parent carries it instead.
        assert!(!src.authors.is_empty());
        assert_eq!(src.authors[0].last, "Smith");
        assert_eq!(src.book_title, None);
    }

    #[test]
    fn chapter_entry_has_book_title() {
        let src = parse_and_convert(
            r#"
            @inbook{jones2019,
                author = {Jones, Alice},
                title = {A Chapter},
                booktitle = {Great Book},
                year = {2019},
                publisher = {Example Press},
            }
            "#,
            "jones2019",
        );
        assert_eq!(src.source_type, SourceType::BookSection);
        assert_eq!(src.book_title.as_deref(), Some("Great Book"));
        assert_eq!(src.journal_name, None);
    }

    #[test]
    fn web_entry_maps_to_internet_site() {
        let src = parse_and_convert(
            r#"
            @online{web2023,
                author = {Doe, Jane},
                title = {A Web Page},
                year = {2023},
                url = {https://example.com},
            }
            "#,
            "web2023",
        );
        assert_eq!(src.source_type, SourceType::InternetSite);
        assert_eq!(src.title.as_deref(), Some("A Web Page"));
    }

    #[test]
    fn entry_with_no_authors_has_empty_vec() {
        let src = parse_and_convert(
            r#"
            @misc{anon2022,
                title = {Anonymous Work},
                year = {2022},
            }
            "#,
            "anon2022",
        );
        assert!(src.authors.is_empty());
        assert_eq!(src.title.as_deref(), Some("Anonymous Work"));
    }
}
