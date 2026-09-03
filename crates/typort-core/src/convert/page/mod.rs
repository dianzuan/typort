//! `PagedDocument` style and page-layout extraction.
//!
//! The submodules mirror the converter's three information sources: rendered
//! frame detection, authoritative source-AST rules, and helpers that reconcile
//! those values onto the document model.

mod hanging_indent;
mod language;
mod margin;
mod reachable;
mod run_style;
mod sections;
mod source_ast;
mod style;
mod units;

use super::{frames, stats};

pub use hanging_indent::{ParHangingRule, collect_par_hanging_indent_rules};
pub use language::localize_cjk_fonts;
pub use margin::{
    MarginsPt, detect_page_numbering, extract_footer, extract_header, extract_page_settings,
};
pub use reachable::{collect_reachable_source_texts, extract_import_paths};
pub use run_style::apply_styles_from_paged;
pub use sections::{DetectedSection, apply_section_breaks, detect_section_breaks};
pub use source_ast::{
    SourceStyleOverrides, extract_show_template_names_from_source, extract_source_style_overrides,
};
pub use style::extract_document_style;

pub(crate) use language::is_cjk_char;
pub(super) use language::lang_region_to_bcp47;
pub(super) use margin::find_body_zone;
pub(super) use style::DEFAULT_BODY_SIZE_HALF_PT;
pub(super) use style::apply_footnote_text_size;
pub(super) use units::{pt_to_eighth_pt, pt_to_half_pt, pt_to_tenths, pt_to_twips};
