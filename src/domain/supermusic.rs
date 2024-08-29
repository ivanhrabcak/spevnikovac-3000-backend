use std::io;

use anyhow::Context;
use itertools::Itertools;
use nom::{
    branch::alt,
    bytes::complete::{take_while1, take_while_m_n},
    character::complete::char,
    combinator::map,
    error::{context, ContextError, ErrorKind, ParseError},
    sequence::delimited,
    IResult,
};
use reqwest::Client;
use scraper::{Html, Selector};

use super::core::{Appendable, LyricsWithChords, TextNode};

pub struct Supermusic {}

impl Supermusic {
    pub fn get(
        document: &scraper::Html,
        txt_export_document: String,
    ) -> anyhow::Result<super::core::LyricsWithChords> {
        let song_name_selector = Selector::parse(".test3").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!")
        })?;

        let whole_song_title = document
            .select(&song_name_selector)
            .nth(0)
            .context("Unexpected document structure!")?
            .text()
            .nth(0)
            .context("No song name in DOM!")?;

        let [artist, song_name] = whole_song_title.split(" - ").collect::<Vec<&str>>()[0..2] else {
            return Err(anyhow::Error::msg("Unexpected structure of song title"));
        };

        let lf_template = txt_export_document.replace("\r\n", "\n");
        let mut song_template: Vec<&str> = lf_template.split("\n").collect();

        // remove whitespace
        song_template.drain(0..2);

        let nodes = match parse_lyrics_with_chords::<(&str, ErrorKind)>(&song_template.join("\n")) {
            Ok(r) => r,
            Err((e, kind)) => {
                return Err(anyhow::Error::msg(format!("{}: {}", kind.description(), e)))
            }
        };

        let mut lines = Vec::new();
        let mut line = Vec::new();
        nodes.iter().for_each(|n| match n {
            TextNode::Newline => {
                lines.push(line.clone());
                line = Vec::new();
            }
            _ => line.push(n.clone()),
        });

        let mut corrected_lines: Vec<Vec<TextNode>> = Vec::new();
        for line in lines {
            let get_text = |n| {
                if let TextNode::Text(t) = n {
                    t
                } else {
                    unreachable!()
                }
            };

            let line_text = get_text(
                line.iter()
                    .map(|n| n.clone())
                    .reduce(|acc, node| {
                        let mut k = TextNode::Text("".to_string());
                        if let TextNode::Text(_) = acc {
                            k = acc.clone()
                        }

                        if let TextNode::Text(t) = node {
                            TextNode::Text(get_text(k) + &t)
                        } else {
                            k
                        }
                    })
                    .unwrap_or(TextNode::Text("".to_string())),
            );

            let mut index = 0;
            let possible_indices: Vec<usize> = line_text
                .split(" ")
                .enumerate()
                .flat_map(|(i, t)| {
                    if i != 0 {
                        index += 1;
                    }

                    let result = vec![index, index + t.len()];
                    index += t.len();

                    result
                })
                .dedup()
                .collect();

            let mut reordered_line = Vec::new();
            let mut incorrect_chords = Vec::new();
            for (i, n) in line.iter().enumerate() {
                reordered_line.push(n.clone());

                if i == 0 {
                    continue;
                }

                let previous = line[i - 1].clone();

                if !matches!(n, &TextNode::Chord(_)) {
                    continue;
                }

                if !matches!(previous, TextNode::Text(_)) {
                    if i != line.len() - 1 {
                        let next = line[i + 1].clone();
                        if let TextNode::Text(t) = next {
                            if t.starts_with(" ") {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                } else if matches!(previous, TextNode::Text(t) if t.ends_with(" ")) {
                    continue;
                }

                // we will be moving this chord as it is not surrounded by spaces
                reordered_line.pop();

                let mut chord_index = 0;
                line.iter().enumerate().for_each(|(k, n)| {
                    if k >= i {
                        return;
                    }

                    if let TextNode::Text(t) = n.clone() {
                        chord_index += t.len()
                    }
                });

                let (_, closest_index) = possible_indices
                    .iter()
                    .map(|k| (chord_index.abs_diff(*k), *k))
                    .sorted_by(|(a_diff, _ind1), (b_diff, _ind2)| Ord::cmp(a_diff, b_diff))
                    .nth(0)
                    .unwrap();

                incorrect_chords.push((closest_index, n.clone()));
            }

            incorrect_chords
                .iter()
                .for_each(|(i, ch)| reordered_line.push_chord(*i, ch.clone()));

            reordered_line = reordered_line
                .iter()
                .map(|n| {
                    if let TextNode::Chord(ch) = n {
                        return TextNode::Chord(ch.replace("Es", "Eb").replace("As", "Ab"));
                    } else {
                        n.clone()
                    }
                })
                .filter(|n| {
                    if let TextNode::Text(t) = n.clone() {
                        return t != "";
                    }

                    true
                })
                .map(|n| n.clone())
                .collect();

            corrected_lines.push(
                reordered_line
                    .iter()
                    .enumerate()
                    .flat_map(|(i, n)| {
                        if i == 0 {
                            return vec![n.clone()];
                        }

                        let previous = reordered_line[i - 1].clone();

                        if matches!(previous, TextNode::Chord(_))
                            && matches!(n, &TextNode::Chord(_))
                        {
                            vec![TextNode::Text(" ".to_string()), n.clone()]
                        } else {
                            vec![n.clone()]
                        }
                    })
                    .collect(),
            );
        }

        Ok(LyricsWithChords::new(
            corrected_lines.join(&TextNode::Newline),
            artist.to_string(),
            song_name.to_string(),
        ))
    }

    pub async fn fetch_whole(url: String) -> anyhow::Result<LyricsWithChords> {
        let song_id = url
            .split("?")
            .nth(1)
            .context("Unexpected url structure!")?
            .split("&")
            .find(|kv| kv.split("=").nth(0).unwrap_or("") == "idpiesne")
            .unwrap_or("=")
            .split("=")
            .nth(1)
            .unwrap();

        let text_export_url = format!(
            "https://supermusic.cz/export.php?idpiesne={}&stiahni=1&typ=TXT&sid=",
            song_id
        );

        let client = Client::new();
        let text_export_response = client.get(text_export_url).send().await?.text().await?;
        let main_document = client.get(url).send().await?.text().await?;

        Self::get(&Html::parse_document(&main_document), text_export_response)
    }
}

fn string<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, &'a str, E> {
    let chars = "\n[]";

    take_while1(move |c| !chars.contains(c))(i)
}

fn chord_block<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "chord",
        map(delimited(char('['), string, char(']')), |o| {
            TextNode::Chord(o.to_string())
        }),
    )(i)
}

fn text<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context("text", map(string, |o| TextNode::Text(o.to_string())))(i)
}

fn newline<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    let newline_take_while = take_while_m_n(1, 1, move |c| c == '\n');
    map(newline_take_while, |_| TextNode::Newline)(i)
}

fn parse_lyrics_with_chords<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> Result<Vec<TextNode>, E> {
    let mut tag_parser = alt((chord_block::<'a, E>, newline::<'a, E>, text::<'a, E>));

    let mut tags = Vec::new();
    let mut s = i;
    while s.len() != 0 {
        let (rest, node) = match tag_parser(s) {
            Ok(r) => r,
            Err(e) => match e {
                nom::Err::Incomplete(_) => {
                    return Err(E::from_error_kind(
                        "Need more data",
                        nom::error::ErrorKind::Eof,
                    ))
                }
                nom::Err::Failure(err) | nom::Err::Error(err) => return Err(err),
            },
        };

        if let TextNode::Chord(ch_streak) = node.clone() {
            if ch_streak.contains(",") {
                let mut nodes: Vec<TextNode> = ch_streak
                    .split(", ")
                    .enumerate()
                    .flat_map(|(i, ch)| {
                        if i != 0 {
                            vec![
                                TextNode::Text(" ".to_string()),
                                TextNode::Chord(ch.to_string()),
                            ]
                        } else {
                            vec![TextNode::Chord(ch.to_string())]
                        }
                    })
                    .collect();

                tags.append(&mut nodes);
            } else {
                tags.push(node);
            }
        } else {
            tags.push(node);
        }

        s = rest;
    }

    Ok(tags)
}
