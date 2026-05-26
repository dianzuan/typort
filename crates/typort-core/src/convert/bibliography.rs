use hayagriva::types::{EntryType, Person};
use typst::comemo::Track;
use typst_html::HtmlDocument;
use typst_library::model::BibliographyElem;
use typort_ooxml::document::{CitationSource, SourceType};

/// Extract bibliography sources from the Typst introspector and map them to
/// Word `CitationSource` entries for the custom XML data store.
///
/// The `Bibliography` type in `typst-library` keeps its entry data private, so
/// we use `BibliographyElem::keys()` to obtain the citation keys and titles,
/// and produce minimal `CitationSource` records.  Full metadata (authors,
/// year, DOI, etc.) would require upstream API changes.
pub fn extract_bibliography_sources(html_doc: &HtmlDocument) -> Vec<CitationSource> {
    let tracked = html_doc.introspector.track();
    let keys = BibliographyElem::keys(tracked);

    keys.into_iter()
        .map(|(label, title_eco)| {
            let tag = label.resolve().to_string();
            let title = title_eco.map(|t| t.to_string());
            CitationSource {
                tag,
                source_type: SourceType::Misc,
                authors: Vec::new(),
                title,
                year: None,
                journal_name: None,
                volume: None,
                issue: None,
                pages: None,
                doi: None,
                url: None,
                publisher: None,
                city: None,
                edition: None,
                book_title: None,
            }
        })
        .collect()
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
