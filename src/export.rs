use docx::{
    document::{BodyContent, Break, BreakType, Paragraph, Run},
    Docx,
};
use itertools::Itertools;
use scraper::Html;
use serde::{Deserialize, Serialize};

use crate::domain::{
    core::{LyricsWithChords, TextNode},
    supermusic::Supermusic,
    ultimate_guitar::UltimateGuitar,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EditingHint {
    Node(TextNode),
    PossibleChordPlace,
}

#[tauri::command(async)]
pub async fn fetch(url: String) -> Result<LyricsWithChords, String> {
    let client = reqwest::Client::new();

    if url.contains("ultimate-guitar.com") {
        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };

        let text = match response.text().await {
            Ok(t) => t,
            Err(e) => return Err(e.to_string()),
        };

        let document = Html::parse_document(&text);

        UltimateGuitar::get(&document, None).map_err(|e| e.to_string())
    } else if url.contains("supermusic.cz") {
        Supermusic::fetch_whole(url)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("This source is not supported!".to_string())
    }
}

#[tauri::command]
pub fn get_editing_hints(nodes: Vec<TextNode>) -> Vec<EditingHint> {
    nodes
        .iter()
        .flat_map(|node| match node {
            TextNode::Text(t) => {
                let parts: Vec<&str> = t.split(" ").collect();

                parts
                    .iter()
                    .enumerate()
                    .flat_map(|(i, part)| {
                        if part.trim() == "" {
                            return vec![EditingHint::Node(TextNode::Text(" ".to_string()))];
                        }

                        let mut p = vec![
                            EditingHint::PossibleChordPlace,
                            EditingHint::Node(TextNode::Text(part.to_string())),
                        ];

                        if i != parts.len() - 1 {
                            p.append(&mut vec![
                                EditingHint::PossibleChordPlace,
                                EditingHint::Node(TextNode::Text(" ".to_string())),
                            ]);
                        }

                        p.push(EditingHint::PossibleChordPlace);

                        p
                    })
                    .collect::<Vec<EditingHint>>()
            }
            TextNode::Chord(_) => vec![
                EditingHint::PossibleChordPlace,
                EditingHint::Node(node.clone()),
                EditingHint::PossibleChordPlace,
            ],
            TextNode::Label(_) => vec![EditingHint::Node(node.clone())],
            TextNode::Newline => vec![EditingHint::Node(node.clone())],
        })
        .dedup_by(|a, b| {
            let is_text_node = |n| matches!(n, &EditingHint::Node(TextNode::Text(_)));
            let is_dedup_node = |n| matches!(n, &EditingHint::PossibleChordPlace);
            let extract_text = |n: &EditingHint| {
                if let EditingHint::Node(TextNode::Text(t)) = n.clone() {
                    t
                } else {
                    unreachable!()
                }
            };

            if !is_text_node(a) || !is_text_node(b) {
                is_dedup_node(a) && is_dedup_node(b)
            } else {
                extract_text(a) == extract_text(b)
            }
        })
        .collect()
}

#[tauri::command]
pub fn write_docx(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String> {
    let mut whole_document = Docx::default();

    for (song_i, song) in songs.iter().enumerate() {
        let song_paragraphs = song.clone().render_docx();
        for (i, paragraph) in song_paragraphs.iter().enumerate() {
            if song_i != 0 && i == 0 {
                let mut p = paragraph.clone();

                // TODO: Make page breaks work
                p.content.insert(
                    0,
                    docx::document::ParagraphContent::Run(
                        Run::default().push_break(Break::from(BreakType::Page)),
                    ),
                );

                whole_document.document.push(p);
            } else {
                whole_document.document.push(paragraph.clone());
            }
        }
    }

    whole_document
        .write_file(path)
        .map_err(|e| match e {
            docx::DocxError::IO(e) => e.to_string(),
            docx::DocxError::Xml(_e) => "Xml Error!".to_string(),
            docx::DocxError::Zip(e) => e.to_string(),
        })
        .map(|_| ())
}

#[tauri::command]
pub fn transpose(nodes: Vec<TextNode>, modifier: i32) -> Vec<TextNode> {
    let mut dummy_lyrics = LyricsWithChords::new(nodes, "".to_string(), "".to_string());

    dummy_lyrics.transpose(modifier);

    return dummy_lyrics.text;
}
