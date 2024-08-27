use docx::document::Text;
use itertools::Itertools;
use scraper::Html;
use serde::{Deserialize, Serialize};

use crate::domain::{
    core::{LyricsWithChords, Source, TextNode},
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
