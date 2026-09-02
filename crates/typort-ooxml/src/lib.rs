#![warn(clippy::pedantic)]

pub mod document;
pub mod styles;
pub mod writer;

pub use document::{
    CitationSource, Document, DocumentMetadata, DocumentStyle, FootnoteFormat, HeaderFooter,
    ImageData, ImageFormat, PageNumberFormat, PersonName, SectionBreak, SectionBreakType,
    SourceType,
};
pub use writer::write_docx;
