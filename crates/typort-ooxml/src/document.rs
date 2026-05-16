#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphStyle {
    Normal,
    Heading(u8),
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
    Math { omml_xml: String },
}

#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

impl Run {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
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
        self.inlines.push(InlineElement::Math { omml_xml });
    }
}

/// A table cell containing paragraphs.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub paragraphs: Vec<Paragraph>,
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
pub struct Document {
    pub body: Body,
    pub page_settings: PageSettings,
    /// Footnotes referenced by `FootnoteRef` inline elements.
    pub footnotes: Vec<Footnote>,
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

    /// Add a footnote and return its 1-based ID.
    pub fn add_footnote(&mut self, content: Vec<Run>) -> u32 {
        let id = u32::try_from(self.footnotes.len()).unwrap_or(u32::MAX - 1) + 1;
        self.footnotes.push(Footnote { id, content });
        id
    }
}
