#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphStyle {
    Normal,
    Heading(u8),
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

#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    pub style: Option<ParagraphStyle>,
}

impl Paragraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_run(&mut self, text: &str) {
        self.runs.push(Run::new(text));
    }
}

#[derive(Debug, Clone)]
pub enum BlockElement {
    Paragraph(Paragraph),
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
}

impl Document {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_paragraph(&mut self, para: Paragraph) {
        self.body.elements.push(BlockElement::Paragraph(para));
    }
}
