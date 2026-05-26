use std::path::Path;

use hayagriva::types::{EntryType, Person};
use typst::comemo::Track;
use typst_html::HtmlDocument;
use typst_library::loading::DataSource;
use typst_library::model::BibliographyElem;
use typort_ooxml::document::{CitationSource, SourceType};

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
    let tracked = html_doc.introspector.track();

    // Find the BibliographyElem to access source file paths
    let Ok(bib_elem) = BibliographyElem::find(tracked) else {
        return Vec::new();
    };

    // Re-parse the bibliography files to get full hayagriva Entry data
    let library = load_bibliography_library(&bib_elem.sources.source.0, world);

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
fn load_bibliography_library(
    sources: &[DataSource],
    world: &TyportWorld,
) -> hayagriva::Library {
    let mut library = hayagriva::Library::new();
    let root = world.root();

    for source in sources {
        match source {
            DataSource::Path(path_str) => {
                let path = root.join(path_str.as_str().trim_start_matches('/'));
                if let Ok(content) = std::fs::read_to_string(&path) {
                    parse_into_library(&mut library, &content, &path);
                }
            }
            DataSource::Bytes(bytes) => {
                if let Ok(content) = std::str::from_utf8(bytes.as_slice()) {
                    // Try BibLaTeX first, then YAML
                    if let Ok(parsed) = hayagriva::io::from_biblatex_str(content) {
                        merge_library(&mut library, parsed);
                    } else if let Ok(parsed) = hayagriva::io::from_yaml_str(content) {
                        merge_library(&mut library, parsed);
                    }
                }
            }
        }
    }

    library
}

/// Parse a bibliography file's content based on its extension.
fn parse_into_library(library: &mut hayagriva::Library, content: &str, path: &Path) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "bib" => {
            if let Ok(parsed) = hayagriva::io::from_biblatex_str(content) {
                merge_library(library, parsed);
            }
        }
        "yaml" | "yml" => {
            if let Ok(parsed) = hayagriva::io::from_yaml_str(content) {
                merge_library(library, parsed);
            }
        }
        _ => {
            // Try BibLaTeX first, then YAML
            if let Ok(parsed) = hayagriva::io::from_biblatex_str(content) {
                merge_library(library, parsed);
            } else if let Ok(parsed) = hayagriva::io::from_yaml_str(content) {
                merge_library(library, parsed);
            }
        }
    }
}

fn merge_library(target: &mut hayagriva::Library, source: hayagriva::Library) {
    for entry in source.iter() {
        target.push(entry);
    }
}

/// Convert a hayagriva Entry to a Word CitationSource with full metadata.
fn entry_to_citation_source(tag: &str, entry: &hayagriva::Entry) -> CitationSource {
    let authors = entry
        .authors()
        .map(|a| a.iter().map(map_person).collect())
        .unwrap_or_default();

    let title = entry.title().map(|t| t.value.to_str().into_owned());
    let year = entry.date().map(|d| d.year.to_string());
    let doi = entry.doi().map(String::from);
    let url = entry.url().map(|u| u.value.to_string());
    let volume = entry.volume().map(|v| v.to_string());
    let issue = entry.issue().map(|i| i.to_string());
    let pages = entry.page_range().map(|p| p.to_string());

    let journal_name = entry
        .parents()
        .first()
        .and_then(|p| p.title())
        .map(|t| t.value.to_str().into_owned());

    let publisher = entry
        .publisher()
        .and_then(|p| p.name())
        .map(|n| n.value.to_str().into_owned());

    let city = entry
        .publisher()
        .and_then(|p| p.location())
        .map(|l| l.value.to_str().into_owned());

    let edition = entry.edition().map(|e| e.to_string());

    let book_title = if matches!(
        entry.entry_type(),
        EntryType::Chapter | EntryType::Anthos
    ) {
        journal_name.clone()
    } else {
        None
    };

    CitationSource {
        tag: tag.to_string(),
        source_type: map_entry_type(entry.entry_type()),
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
pub fn map_entry_type(entry_type: &EntryType) -> SourceType {
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
            map_entry_type(&EntryType::Article),
            SourceType::JournalArticle
        );
    }

    #[test]
    fn book_maps_to_book() {
        assert_eq!(map_entry_type(&EntryType::Book), SourceType::Book);
    }

    #[test]
    fn thesis_maps_to_thesis() {
        assert_eq!(map_entry_type(&EntryType::Thesis), SourceType::Thesis);
    }

    #[test]
    fn web_maps_to_internet_site() {
        assert_eq!(
            map_entry_type(&EntryType::Web),
            SourceType::InternetSite
        );
    }

    #[test]
    fn unknown_maps_to_misc() {
        assert_eq!(map_entry_type(&EntryType::Video), SourceType::Misc);
    }

    #[test]
    fn person_maps_name_fields() {
        let person = Person {
            name: "Smith".into(),
            given_name: Some("John".into()),
            prefix: None,
            suffix: None,
            alias: None,
        };
        let mapped = map_person(&person);
        assert_eq!(mapped.last, "Smith");
        assert_eq!(mapped.first.as_deref(), Some("John"));
    }
}
