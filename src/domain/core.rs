use docx::{
    document::{Paragraph, Run, Text, TextSpace},
    formatting::{CharacterProperty, VerticalAlignment},
    Docx,
};

#[derive(Clone, Debug)]
pub struct LyricsWithChords {
    text: Vec<TextNode>,
}

impl LyricsWithChords {
    pub fn new(text: Vec<TextNode>) -> Self {
        Self { text }
    }

    pub fn render(&self) -> Docx {
        let mut doc = Docx::default();

        let mut paragraph = Paragraph::default();
        for node in self.text.clone() {
            match node {
                TextNode::Text(t) => {
                    paragraph = paragraph
                        .push(Run::default().push_text(Text::from((t, TextSpace::Preserve))))
                }
                TextNode::Chord(ch) => {
                    paragraph = paragraph.push(
                        Run::default()
                            .push_text(Text::from((ch, TextSpace::Preserve)))
                            .property(
                                CharacterProperty::default()
                                    .bold(true)
                                    .vertical_alignment(VerticalAlignment::superscript()),
                            ),
                    )
                }
                TextNode::Label(l) => {
                    paragraph = paragraph.push(
                        Run::default()
                            .push_text(l)
                            .property(CharacterProperty::default().bold(true)),
                    )
                }
                TextNode::Newline => {
                    doc.document.push(paragraph);
                    paragraph = Paragraph::default();
                }
            };
        }

        if paragraph.content.len() != 0 {
            doc.document.push(paragraph);
        }

        doc
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextNode {
    Text(String),
    Chord(String),
    Label(String),
    Newline,
}
