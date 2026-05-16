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
        }
    }
}

/// A footnote with its content paragraphs.
#[derive(Debug, Clone)]
pub struct Footnote {
    /// 1-based footnote ID.
    pub id: u32,
    /// The text content of the footnote (collected as paragraph runs).
    pub content: Vec<Run>,
}

#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    /// Inline elements including text runs and footnote references.
    pub inlines: Vec<InlineElement>,
    pub style: Option<ParagraphStyle>,
    /// If this paragraph is a list item, the nesting level (0-based).
    pub list_level: Option<u32>,
    /// If this paragraph is a list item, the numbering definition ID.
    /// Use 1 for ordered lists, 2 for unordered lists.
    pub list_id: Option<u32>,
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
}

impl Paragraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_run(&mut self, text: &str) {
        let run = Run::new(text);
        self.inlines.push(InlineElement::Text(run.clone()));
        self.runs.push(run);
    }

    /// Add a pre-built run to this paragraph.
    pub fn push_run(&mut self, run: Run) {
        self.inlines.push(InlineElement::Text(run.clone()));
        self.runs.push(run);
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
}

/// A table cell containing paragraphs.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub paragraphs: Vec<Paragraph>,
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
}

#[derive(Debug, Clone)]
pub enum BlockElement {
    Paragraph(Paragraph),
    Table(Table),
}

#[derive(Debug, Clone, Default)]
pub struct Body {
    pub elements: Vec<BlockElement>,
}

#[derive(Debug, Clone)]
pub struct PageSettings {
    pub width_twips: u32,
    pub height_twips: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
    pub margin_left: u32,
    pub margin_right: u32,
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
    /// Falls back to a compile-time default if not explicitly set.
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

#[derive(Debug, Clone)]
pub struct DocumentStyle {
    pub body_font_ascii: String,
    pub body_font_east_asia: String,
    pub body_size_half_pt: u32,
    pub line_spacing: u32,
    pub first_line_indent_twips: u32,
    pub footnote_format: FootnoteFormat,
}

impl Default for DocumentStyle {
    fn default() -> Self {
        Self {
            body_font_ascii: "Times New Roman".to_string(),
            body_font_east_asia: "\u{5b8b}\u{4f53}".to_string(),
            body_size_half_pt: 21,
            line_spacing: 360,
            first_line_indent_twips: 420,
            footnote_format: FootnoteFormat::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub body: Body,
    pub page_settings: PageSettings,
    /// Footnotes referenced by `FootnoteRef` inline elements.
    pub footnotes: Vec<Footnote>,
    /// Document metadata (title, author) written to docProps/core.xml.
    pub metadata: DocumentMetadata,
    /// Style information extracted from the Typst document rendering.
    pub style: DocumentStyle,
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

    /// Add a footnote and return its ID (starting from 2, since 0 and 1 are reserved by OOXML).
    pub fn add_footnote(&mut self, content: Vec<Run>) -> u32 {
        let id = u32::try_from(self.footnotes.len()).unwrap_or(u32::MAX - 3) + 2;
        self.footnotes.push(Footnote { id, content });
        id
    }
}
