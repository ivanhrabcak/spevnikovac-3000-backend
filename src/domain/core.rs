use anyhow::Result;
use docx::{
    document::{Paragraph, Run, Text, TextSpace},
    formatting::{CharacterProperty, VerticalAlignment},
    Docx,
};
use scraper::Html;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LyricsWithChords {
    pub text: Vec<TextNode>,
    pub artist: String,
    pub song_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub chorus_label: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            chorus_label: "®:".to_string(),
        }
    }
}

pub trait Source {
    fn get(document: &Html, options: Option<Options>) -> Result<LyricsWithChords>;
}

impl LyricsWithChords {
    pub fn new(text: Vec<TextNode>, artist: String, song_name: String) -> Self {
        Self {
            text,
            artist,
            song_name,
        }
    }

    pub fn render_docx(&self) -> Docx {
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

pub trait Appendable {
    fn push_chord(&mut self, position: usize, node: TextNode);
}

impl Appendable for Vec<TextNode> {
    fn push_chord(&mut self, position: usize, chord: TextNode) {
        let mut character_index = 0;
        for (i, node) in self.clone().iter().enumerate() {
            let text = if let TextNode::Text(t) = node {
                t
            } else {
                continue;
            };

            // The chord belongs to this part of the text
            if position >= character_index && position <= character_index + text.len() {
                let start = text[0..position - character_index].to_string();
                let end = text[position - character_index..text.len()].to_string();

                if end.trim_end() == "" {
                    let mut node_insert_index = i + 1;
                    while matches!(self.get(node_insert_index), Some(TextNode::Chord(_)))
                        && node_insert_index < self.len()
                    {
                        node_insert_index += 1;
                    }

                    if node_insert_index == self.len() {
                        self.push(chord);
                        return;
                    } else {
                        self.insert(node_insert_index, chord);
                        return;
                    }
                }

                self.remove(i);

                self.insert(i, TextNode::Text(end));
                self.insert(i, chord);
                self.insert(i, TextNode::Text(start));
                return;
            }

            character_index += text.len();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextNode {
    Text(String),
    Chord(String),
    Label(String),
    Newline,
}
