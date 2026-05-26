#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphStyle {
    Normal,
    Heading(u8),
}

/// Paragraph alignment / justification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    Justify,
}

impl Alignment {
    /// Return the OOXML `w:jc` value string for this alignment.
    #[must_use]
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
            Alignment::Justify => "both",
        }
    }
}

/// Image format (PNG or JPEG).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Embedded image data with dimensions in EMU (English Metric Units).
#[derive(Debug, Clone)]
pub struct ImageData {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
    pub width_emu: u64,
    pub height_emu: u64,
}

/// List numbering info for a paragraph that is a list item.
#[derive(Debug, Clone)]
pub struct ListInfo {
    /// Nesting level (0-based).
    pub level: u32,
    /// Numbering definition ID (e.g. 1=ordered, 2=unordered).
    pub id: u32,
}

/// Word bibliography source type (ST_SourceType).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    JournalArticle,
    Book,
    BookSection,
    ConferenceProceedings,
    Report,
    Thesis,
    InternetSite,
    DocumentFromInternetSite,
    Misc,
}

impl SourceType {
    #[must_use]
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Self::JournalArticle => "JournalArticle",
            Self::Book => "Book",
            Self::BookSection => "BookSection",
            Self::ConferenceProceedings => "ConferenceProceedings",
            Self::Report => "Report",
            Self::Thesis => "Thesis",
            Self::InternetSite => "InternetSite",
            Self::DocumentFromInternetSite => "DocumentFromInternetSite",
            Self::Misc => "Misc",
        }
    }
}

/// A person's name for Word bibliography author fields.
#[derive(Debug, Clone)]
pub struct PersonName {
    pub last: String,
    pub first: Option<String>,
    pub middle: Option<String>,
}

/// A citation source entry for Word's bibliography data store (`customXml/item1.xml`).
#[derive(Debug, Clone)]
pub struct CitationSource {
    pub tag: String,
    pub source_type: SourceType,
    pub authors: Vec<PersonName>,
    pub title: Option<String>,
    pub year: Option<String>,
    pub journal_name: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub publisher: Option<String>,
    pub city: Option<String>,
    pub edition: Option<String>,
    pub book_title: Option<String>,
}

/// A single inline element within a paragraph.
#[derive(Debug, Clone)]
pub enum InlineElement {
    /// Normal text run.
    Text(Run),
    /// A footnote reference (rendered as superscript number in the document).
    /// The `id` corresponds to the footnote in the `Document::footnotes` list.
    FootnoteRef(u32),
    /// A math equation rendered as OMML XML.
    /// If `equation_number` is set, the equation is a numbered block equation.
    Math {
        omml_xml: String,
        equation_number: Option<String>,
    },
    /// An inline image.
    Image(ImageData),
    /// A bookmark start marker (anchor for cross-references).
    Bookmark { id: u32, name: String },
    /// A bookmark end marker.
    BookmarkEnd { id: u32 },
    /// A cross-reference field (REF field code pointing at a bookmark).
    FieldRef {
        bookmark_name: String,
        display_text: String,
    },
    /// An external hyperlink using `fldSimple` with HYPERLINK field code.
    Hyperlink { url: String, runs: Vec<Run> },
    /// A page break (`w:br type="page"`).
    PageBreak,
    /// A Table of Contents field code (`TOC \o "1-N" \h \z \u`).
    FieldToc {
        /// Maximum outline depth (e.g. 3 → headings 1–3).
        max_depth: u8,
    },
    /// A tab character (`w:r` containing `w:tab`).
    Tab,
    /// A Word citation wrapped in a Structured Document Tag (SDT).
    Citation {
        /// Citation keys (multiple for merged citations like "[1, 3]").
        keys: Vec<String>,
        /// Rendered display text (e.g., "[1]" or "(Author, 2020)").
        display_text: String,
    },
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub monospace: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Highlight color name (e.g. "yellow", "green", "cyan").
    /// `Some(...)` activates highlighting; `None` means no highlight.
    pub highlight_color: Option<String>,
    pub smallcaps: bool,
    /// Text color as a 6-digit hex string (e.g. "FF0000" for red).
    /// `None` means inherit the default (black).
    pub color: Option<String>,
    /// Per-run font override for ASCII/Latin text (e.g. from show rules).
    /// `None` means inherit the document default.
    pub font_ascii: Option<String>,
    /// Per-run font override for CJK text.
    pub font_east_asia: Option<String>,
    /// Per-run font size override in half-points (e.g. 24 = 12pt).
    /// `None` means inherit the document default.
    pub size_half_pt: Option<u32>,
    /// Source span for cross-referencing with `PagedDocument` styling.
    pub span: Option<typst_syntax::Span>,
}

impl Run {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            superscript: false,
            subscript: false,
            monospace: false,
            underline: false,
            strikethrough: false,
            highlight_color: None,
            smallcaps: false,
            color: None,
            font_ascii: None,
            font_east_asia: None,
            size_half_pt: None,
            span: None,
        }
    }
}

/// A footnote with its content paragraphs.
#[derive(Debug, Clone)]
pub struct Footnote {
    /// 1-based footnote ID.
    pub id: u32,
    /// The inline content of the footnote (text runs, math, etc.).
    pub content: Vec<InlineElement>,
}

#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Paragraph {
    /// Inline elements including text runs and footnote references.
    pub inlines: Vec<InlineElement>,
    pub style: Option<ParagraphStyle>,
    /// If this paragraph is a list item, its numbering info (level + ID).
    pub list_info: Option<ListInfo>,
    /// Paragraph alignment (left, center, right, justify).
    pub alignment: Option<Alignment>,
    /// Suppress first-line indent (e.g., first paragraph after a heading).
    pub suppress_indent: bool,
    /// Use hanging indent (e.g., bibliography entries).
    pub hanging_indent: bool,
    /// Left indent in twips (e.g., 720 for block quotes).
    pub left_indent: Option<u32>,
    /// Apply `CodeBlock` style (monospace, no indent, optional shading).
    pub code_block: bool,
    /// If set, this paragraph ends a section. The `w:sectPr` is emitted
    /// inside this paragraph's `w:pPr`.
    pub section_break: Option<SectionBreak>,
    /// If true, this paragraph represents a horizontal rule (bottom border).
    pub horizontal_rule: bool,
    /// Tab stop positions in twips (e.g., for grid/multi-column recovery).
    /// Emitted as `<w:tabs><w:tab w:val="right" w:pos="..."/></w:tabs>` in `w:pPr`.
    pub tab_stops: Vec<u32>,
    /// Override the paragraph's `w:before` spacing (twips).
    /// Used to suppress heading above-spacing at the start of a page/document.
    pub spacing_before: Option<u32>,
}

impl Paragraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_run(&mut self, text: &str) {
        let run = Run::new(text);
        self.inlines.push(InlineElement::Text(run));
    }

    /// Add a pre-built run to this paragraph.
    pub fn push_run(&mut self, run: Run) {
        self.inlines.push(InlineElement::Text(run));
    }

    /// Get concatenated text content from all text runs.
    #[must_use]
    pub fn text_content(&self) -> String {
        self.inlines
            .iter()
            .filter_map(|i| {
                if let InlineElement::Text(run) = i {
                    Some(run.text.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    #[must_use]
    pub fn full_text_content(&self) -> String {
        let mut text = String::new();
        for inline in &self.inlines {
            match inline {
                InlineElement::Text(run) => text.push_str(&run.text),
                InlineElement::Math { omml_xml, .. } => {
                    for part in omml_xml.split("<m:t>") {
                        if let Some(end) = part.find("</m:t>") {
                            text.push_str(&part[..end]);
                        }
                    }
                }
                InlineElement::Hyperlink { runs, .. } => {
                    for run in runs {
                        text.push_str(&run.text);
                    }
                }
                InlineElement::FieldRef { display_text, .. }
                | InlineElement::Citation { display_text, .. } => {
                    text.push_str(display_text);
                }
                _ => {}
            }
        }
        text
    }

    /// Iterate over text runs (immutable).
    pub fn text_runs(&self) -> impl Iterator<Item = &Run> + '_ {
        self.inlines.iter().filter_map(|i| {
            if let InlineElement::Text(run) = i {
                Some(run)
            } else {
                None
            }
        })
    }

    /// Iterate over text runs (mutable).
    pub fn text_runs_mut(&mut self) -> impl Iterator<Item = &mut Run> + '_ {
        self.inlines.iter_mut().filter_map(|i| {
            if let InlineElement::Text(run) = i {
                Some(run)
            } else {
                None
            }
        })
    }

    /// Add a footnote reference to this paragraph.
    pub fn add_footnote_ref(&mut self, id: u32) {
        self.inlines.push(InlineElement::FootnoteRef(id));
    }

    /// Add a math equation (OMML XML) to this paragraph.
    pub fn add_math(&mut self, omml_xml: String) {
        self.inlines.push(InlineElement::Math {
            omml_xml,
            equation_number: None,
        });
    }

    /// Add a numbered math equation (OMML XML) to this paragraph.
    pub fn add_numbered_math(&mut self, omml_xml: String, number: String) {
        self.inlines.push(InlineElement::Math {
            omml_xml,
            equation_number: Some(number),
        });
    }

    /// Add an inline image to this paragraph.
    pub fn add_image(&mut self, image: ImageData) {
        self.inlines.push(InlineElement::Image(image));
    }

    /// Add a bookmark start + end pair (anchor for cross-references).
    pub fn add_bookmark(&mut self, id: u32, name: String) {
        self.inlines.push(InlineElement::Bookmark { id, name });
        self.inlines.push(InlineElement::BookmarkEnd { id });
    }

    /// Add a cross-reference field (REF field code).
    pub fn add_field_ref(&mut self, bookmark_name: String, display_text: String) {
        self.inlines.push(InlineElement::FieldRef {
            bookmark_name,
            display_text,
        });
    }

    /// Add an external hyperlink.
    pub fn add_hyperlink(&mut self, url: String, runs: Vec<Run>) {
        self.inlines.push(InlineElement::Hyperlink { url, runs });
    }

    /// Add a Table of Contents field code.
    pub fn add_toc(&mut self, max_depth: u8) {
        self.inlines.push(InlineElement::FieldToc { max_depth });
    }

    /// Add a page break.
    pub fn add_page_break(&mut self) {
        self.inlines.push(InlineElement::PageBreak);
    }

    /// Add a tab character.
    pub fn add_tab(&mut self) {
        self.inlines.push(InlineElement::Tab);
    }

    /// Add a citation field (SDT-wrapped CITATION field code).
    pub fn add_citation(&mut self, keys: Vec<String>, display_text: String) {
        self.inlines.push(InlineElement::Citation { keys, display_text });
    }
}

/// Content of a table cell: a mix of paragraphs and nested tables in order.
#[derive(Debug, Clone)]
pub enum CellContent {
    Paragraph(Paragraph),
    Table(Table),
}

/// A table cell containing paragraphs and optionally nested tables.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub paragraphs: Vec<Paragraph>,
    /// Ordered cell content (paragraphs + nested tables). When non-empty, this
    /// is used instead of `paragraphs` for serialisation, allowing cells to
    /// contain nested `<w:tbl>` elements interleaved with `<w:p>` elements.
    pub content: Vec<CellContent>,
    /// Number of columns this cell spans (1 = no merge). Maps to `w:gridSpan`.
    pub colspan: u32,
    /// Vertical merge state. Maps to `w:vMerge`.
    pub vmerge: VMerge,
    /// Cell width as percentage of table width (in fiftieths of a percent, i.e. 5000 = 100%).
    /// If None, width is auto-distributed.
    pub width_pct: Option<u32>,
}

/// Vertical merge state for a table cell.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VMerge {
    /// Not part of a vertical merge.
    #[default]
    None,
    /// Start (first cell) of a vertical merge (`w:vMerge val="restart"`).
    Restart,
    /// Continuation cell of a vertical merge (`w:vMerge` with no val).
    Continue,
}

/// A table row containing cells.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

/// A table with rows and cells.
#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<TableRow>,
    /// Table width as percentage (in fiftieths of a percent, e.g. 5000 = 100%).
    /// `None` defaults to 5000 (100%).
    pub width_pct: Option<u32>,
    /// Border size in eighths of a point (e.g. 4 = 0.5pt).
    /// `None` defaults to 4 (0.5pt solid).
    pub border_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum BlockElement {
    Paragraph(Paragraph),
    Table(Table),
    /// Bibliography section wrapped in SDT with BIBLIOGRAPHY field code.
    BibliographyBlock { paragraphs: Vec<Paragraph> },
}

#[derive(Debug, Clone, Default)]
pub struct Body {
    pub elements: Vec<BlockElement>,
}

/// Section break type for multi-section documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionBreakType {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
}

/// A section break that ends a section, optionally overriding page settings.
#[derive(Debug, Clone)]
pub struct SectionBreak {
    pub break_type: SectionBreakType,
    pub page_settings: Option<PageSettings>,
}

/// Header or footer content (a sequence of paragraphs).
#[derive(Debug, Clone)]
pub struct HeaderFooter {
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone)]
pub struct PageSettings {
    pub width_twips: u32,
    pub height_twips: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    /// Number of columns in this section (None or 1 = single column).
    pub columns: Option<u32>,
    /// Spacing between columns in twips (default 720 = 0.5 inch).
    pub column_spacing: Option<u32>,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            width_twips: 11906,  // A4 width
            height_twips: 16838, // A4 height
            margin_top: 1440,    // 2.54cm
            margin_bottom: 1440,
            margin_left: 1800, // 3.17cm
            margin_right: 1800,
            columns: None,
            column_spacing: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    /// ISO 8601 timestamp for creation time. If not set, a default is used.
    pub created: Option<String>,
}

impl DocumentMetadata {
    /// Return the creation timestamp string (ISO 8601 / W3CDTF format).
    /// Falls back to a fixed default if not explicitly set.
    ///
    /// NOTE: We use a fixed fallback rather than a compile-time timestamp
    /// (`env!("BUILD_TIMESTAMP")`) to avoid adding a build-script dependency
    /// and to keep builds reproducible.
    #[must_use]
    pub fn created_time(&self) -> String {
        self.created
            .clone()
            .unwrap_or_else(|| "2026-01-01T00:00:00Z".to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FootnoteFormat {
    #[default]
    Decimal,
    CircledNumber,
}

/// Page number format for footer page numbering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageNumberFormat {
    Decimal,
    LowerRoman,
    UpperRoman,
    LowerLetter,
    UpperLetter,
}

#[derive(Debug, Clone)]
pub struct DocumentStyle {
    pub body_font_ascii: String,
    pub body_font_east_asia: String,
    pub body_size_half_pt: u32,
    pub line_spacing: u32,
    pub first_line_indent_twips: u32,
    pub footnote_format: FootnoteFormat,
    /// Font used for code/raw blocks (detected from Typst rendering).
    pub code_font: String,
    /// Font size for code blocks in half-points.
    pub code_size_half_pt: u32,
    /// Font size for footnote text in half-points.
    pub footnote_size_half_pt: u32,
    /// Heading sizes in half-points, indexed by level (0=h1, 1=h2, ..., 4=h5).
    pub heading_sizes: [u32; 5],
    /// Paragraph justification (e.g., "both" for justify, "left" for left-align).
    pub body_alignment: String,
    /// Body paragraph spacing before in twips.
    pub body_spacing_before: u32,
    /// Body paragraph spacing after in twips.
    pub body_spacing_after: u32,
    /// Heading spacing before in twips, per level (0=h1 .. 4=h5).
    pub heading_spacing_before: [u32; 5],
    /// Heading spacing after in twips, per level (0=h1 .. 4=h5).
    pub heading_spacing_after: [u32; 5],
    /// BCP 47 language tag for Latin text (e.g. "en-US").
    pub lang_latin: String,
    /// BCP 47 language tag for East Asian text (e.g. "zh-CN", "ja-JP", "ko-KR").
    pub lang_east_asia: String,
    /// Whether the document contains CJK content. Controls `w:hint="eastAsia"`.
    pub has_cjk_content: bool,
    /// Hyperlink color as a 6-digit hex string (e.g. "0563C1").
    pub hyperlink_color: String,
    /// Body font's cap-height ratio (em units). Used to compute line pitch:
    /// `line_pitch = cap_height_ratio × body_size + leading`.
    pub body_cap_height_ratio: f64,
}

impl Default for DocumentStyle {
    fn default() -> Self {
        // Typst defaults: 11pt body, 0.65em leading → 18.15pt line height → 363 twips,
        // no first-line indent, h1=1.4em=31hp, h2=1.2em=26hp, h3-5=body size,
        // paragraph spacing = 1.2 * 11pt = 13.2pt → 264 twips.
        Self {
            body_font_ascii: "Times New Roman".to_string(),
            body_font_east_asia: "\u{5b8b}\u{4f53}".to_string(),
            body_size_half_pt: 22,       // Typst default: 11pt = 22 half-points
            line_spacing: 276,           // ~13.8pt: typical rendered line pitch for 11pt body
            first_line_indent_twips: 0,  // Typst default: no indent
            footnote_format: FootnoteFormat::default(),
            code_font: "Courier New".to_string(),
            code_size_half_pt: 22,       // Typst raw text uses body size by default
            footnote_size_half_pt: 18,
            // Typst defaults: h1=1.4*22=31, h2=1.2*22=26, h3-h5=body size
            heading_sizes: [31, 26, 22, 22, 22],
            body_alignment: "left".to_string(),
            // Typst default par.spacing = 1.2em; at 11pt → 13.2pt → 264 twips.
            // before=0 because Word sums before+after (no collapsing).
            body_spacing_before: 0,
            body_spacing_after: 264,
            heading_spacing_before: [0; 5],
            heading_spacing_after: [264; 5],
            lang_latin: "en-US".to_string(),
            lang_east_asia: "zh-CN".to_string(),
            has_cjk_content: true,
            hyperlink_color: "0563C1".to_string(),
            body_cap_height_ratio: 0.66,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub body: Body,
    pub page_settings: PageSettings,
    /// Footnotes referenced by `FootnoteRef` inline elements.
    pub footnotes: Vec<Footnote>,
    /// Document metadata (title, author) written to docProps/core.xml.
    pub metadata: DocumentMetadata,
    /// Style information extracted from the Typst document rendering.
    pub style: DocumentStyle,
    /// Counter for generating unique bookmark IDs.
    pub bookmark_counter: u32,
    /// Default header for the document (written to `word/header1.xml`).
    pub header: Option<HeaderFooter>,
    /// Default footer for the document (written to `word/footer1.xml`).
    pub footer: Option<HeaderFooter>,
    /// If set, footer contains a PAGE field code with this number format.
    /// When this is `Some`, a footer is always generated (even if `footer` is `None`).
    pub page_numbering: Option<PageNumberFormat>,
    /// Counter for generating unique list numbering IDs (starts at 4;
    /// 1=ordered, 2=unordered, 3=Chinese headings are reserved).
    pub next_list_id: u32,
    /// Mapping of dynamically allocated list IDs to their abstract numbering
    /// definition (1=ordered, 2=unordered). Used to generate `w:num` entries.
    pub list_num_instances: Vec<(u32, u32)>,
    /// Bibliography sources for Word's citation data store (customXml/item1.xml).
    pub citation_sources: Vec<CitationSource>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            body: Body::default(),
            page_settings: PageSettings::default(),
            footnotes: Vec::new(),
            metadata: DocumentMetadata::default(),
            style: DocumentStyle::default(),
            bookmark_counter: 0,
            header: None,
            footer: None,
            page_numbering: None,
            next_list_id: 4,
            list_num_instances: Vec::new(),
            citation_sources: Vec::new(),
        }
    }
}

impl Document {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_paragraph(&mut self, para: Paragraph) {
        self.body.elements.push(BlockElement::Paragraph(para));
    }

    pub fn add_table(&mut self, table: Table) {
        self.body.elements.push(BlockElement::Table(table));
    }

    /// Allocate and return the next unique bookmark ID.
    pub fn next_bookmark_id(&mut self) -> u32 {
        let id = self.bookmark_counter;
        self.bookmark_counter += 1;
        id
    }

    /// Allocate a unique list numbering ID for a new top-level list.
    /// `ordered` selects abstractNum 1 (decimal) or 2 (bullet).
    pub fn allocate_list_id(&mut self, ordered: bool) -> u32 {
        let id = self.next_list_id;
        self.next_list_id += 1;
        let abstract_num = if ordered { 1 } else { 2 };
        self.list_num_instances.push((id, abstract_num));
        id
    }

    /// Add a footnote and return its ID (starting from 2, since 0 and 1 are reserved by OOXML).
    pub fn add_footnote(&mut self, content: Vec<InlineElement>) -> u32 {
        let id = u32::try_from(self.footnotes.len()).unwrap_or(u32::MAX - 3) + 2;
        self.footnotes.push(Footnote { id, content });
        id
    }
}
