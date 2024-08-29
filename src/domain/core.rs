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

impl LyricsWithChords {
    pub fn new(text: Vec<TextNode>, artist: String, song_name: String) -> Self {
        Self {
            text,
            artist,
            song_name,
        }
    }

    pub fn render_docx<'a>(self) -> Vec<Paragraph<'a>> {
        let mut paragraphs = Vec::new();

        let title_paragraph = Paragraph::default().push(
            Run::default()
                .push_text(Text::from((
                    format!("{} - {}", self.artist, self.song_name),
                    TextSpace::Preserve,
                )))
                .property(CharacterProperty::default().bold(true)),
        );

        paragraphs.push(title_paragraph);

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
                    paragraphs.push(paragraph);
                    paragraph = Paragraph::default();
                }
            };
        }

        if paragraph.content.len() != 0 {
            paragraphs.push(paragraph);
        }

        paragraphs
    }

    fn transpose_chord(chord: String, modifier: i32) -> String {
        let mut transposed_chord: String;

        let mut chord_type = 0;
        if chord.starts_with("C#") {
            chord_type = 1;
        } else if chord.starts_with("D#") {
            chord_type = 3;
        } else if chord.starts_with("Eb") {
            if !chord.starts_with("Ebsu") {
                chord_type = 3;
            } else {
                chord_type = 4;
            }
        } else if chord.starts_with("F#") {
            chord_type = 6;
        } else if chord.starts_with("G#") {
            chord_type = 8;
        } else if chord.starts_with("Ab") {
            if !chord.starts_with("Absu") {
                chord_type = 8;
            } else {
                chord_type = 9;
            }
        } else if chord.starts_with("A#") {
            chord_type = 10;
        } else if chord.starts_with("Bb") {
            chord_type = 10;
        }

        if chord_type == 0 {
            chord_type = match chord.chars().nth(0).unwrap() {
                'C' => 0,
                'D' => 2,
                'E' => 4,
                'F' => 5,
                'G' => 7,
                'A' => 9,
                'B' => 10,
                'H' => 11,
                _ => unreachable!(),
            };

            transposed_chord = chord[1..chord.len()].to_string();
        } else {
            transposed_chord = chord[2..chord.len()].to_string();
        }

        chord_type += modifier;

        if chord_type > 11 {
            chord_type -= 12;
        } else if chord_type < 0 {
            chord_type += 12;
        }

        transposed_chord = match chord_type {
            0 => "C".to_string() + &transposed_chord,
            1 => "C#".to_string() + &transposed_chord,
            2 => "D".to_string() + &transposed_chord,
            3 => "Eb".to_string() + &transposed_chord,
            4 => "E".to_string() + &transposed_chord,
            5 => "F".to_string() + &transposed_chord,
            6 => "F#".to_string() + &transposed_chord,
            7 => "G".to_string() + &transposed_chord,
            8 => "Ab".to_string() + &transposed_chord,
            9 => "A".to_string() + &transposed_chord,
            10 => "B".to_string() + &transposed_chord,
            11 => "H".to_string() + &transposed_chord,
            _ => unreachable!(),
        };

        if chord.contains("/") {
            let mut parts = chord.split("/").collect::<Vec<&str>>();

            transposed_chord = parts.remove(0).to_string()
                + "/"
                + &parts
                    .iter()
                    .map(|chord| Self::transpose_chord(chord.to_string(), modifier))
                    .collect::<Vec<String>>()
                    .join("/");
        }

        transposed_chord
    }

    pub fn transpose(&mut self, modifier: i32) {
        self.text = self
            .text
            .iter()
            .map(|n| {
                if !matches!(n, &TextNode::Chord(_)) {
                    return n.clone();
                }

                let get_chord = |n: &TextNode| {
                    if let TextNode::Chord(ch) = n.clone() {
                        ch
                    } else {
                        unreachable!()
                    }
                };

                let chord = get_chord(n);

                TextNode::Chord(Self::transpose_chord(chord, modifier))
            })
            .collect()
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
