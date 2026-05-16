#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

impl Paragraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_run(&mut self, text: &str) {
        self.runs.push(Run {
            text: text.to_string(),
        });
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

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub body: Body,
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
