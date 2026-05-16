/// A text run within a paragraph.
#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
}

/// A paragraph containing one or more runs.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

impl Paragraph {
    pub fn new() -> Self {
        Self { runs: Vec::new() }
    }

    pub fn add_run(&mut self, text: &str) {
        self.runs.push(Run {
            text: text.to_string(),
        });
    }
}

impl Default for Paragraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A block-level element in the document body.
#[derive(Debug, Clone)]
pub enum BlockElement {
    Paragraph(Paragraph),
}

/// The document body containing block elements.
#[derive(Debug, Clone)]
pub struct Body {
    pub elements: Vec<BlockElement>,
}

impl Body {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::new()
    }
}

/// The top-level document structure.
#[derive(Debug, Clone)]
pub struct Document {
    pub body: Body,
}

impl Document {
    pub fn new() -> Self {
        Self { body: Body::new() }
    }

    pub fn add_paragraph(&mut self, para: Paragraph) {
        self.body.elements.push(BlockElement::Paragraph(para));
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}
