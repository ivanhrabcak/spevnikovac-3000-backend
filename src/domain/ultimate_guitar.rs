use std::{collections::HashMap, io};

use anyhow::{Context, Error};
use itertools::Itertools;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1, take_while_m_n},
    character::complete::char,
    combinator::{cut, map},
    error::{context, ContextError, ErrorKind, ParseError},
    sequence::{delimited, preceded, terminated},
    IResult,
};
use scraper::{Html, Selector};
use serde_json::Value;

use super::core::{Appendable, LyricsWithChords, Options, Source, TextNode};

pub struct RawParsedData {
    pub artist: String,
    pub song_name: String,
    pub tab_view: String,
}

pub struct UltimateGuitar;

impl UltimateGuitar {
    const CHORD_CHARACTER_WIDTH: usize = 3;

    fn parse_data_from_dom(document: &Html) -> anyhow::Result<RawParsedData> {
        let selector = Selector::parse(".js-store").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!")
        })?;

        let elem = document
            .select(&selector)
            .nth(0)
            .context("Unexpected document structure!")?;

        let data_content_attribute = elem
            .attr("data-content")
            .context("Missing data-content attribute!")?;

        let content = html_escape::decode_html_entities(data_content_attribute)
            .to_string()
            .replace("\\\\", "\\");

        let parsed_content: HashMap<String, Value> = serde_json::from_str(&content).unwrap();

        //song_name": String("Just"), "artist_id": Number(578), "artist_name": String("Radiohead"),

        let page_data = parsed_content
            .get("store")
            .context("Unexpected DOM structure! (store)")?
            .get("page")
            .context("Unexpected DOM structure! (page)")?
            .get("data")
            .context("Unexpected DOM structure! (data)")?;

        let tab_info = page_data.get("tab").context("Failed to get tab info!")?;

        let artist = tab_info
            .get("artist_name")
            .context("Missing artist name!")?
            .as_str()
            .context("Failed to convert artist name to string!")?
            .to_string();

        let song_name = tab_info
            .get("song_name")
            .context("Missing song name!")?
            .as_str()
            .context("Failed to convert song name to string!")?
            .to_string();
        // println!("{}", content);

        let tab_view = page_data
            .get("tab_view")
            .context("Unexpected DOM structure! (tab_view)")?
            .get("wiki_tab")
            .context("Unexpected DOM structure! (wiki_tab)")?
            .get("content")
            .context("Unexpected DOM structure! (content)")?
            .as_str()
            .context("Unexpected content value type!")?
            .to_string();

        return Ok(RawParsedData {
            artist,
            song_name,
            tab_view,
        });
    }
}

impl Source for UltimateGuitar {
    fn get(document: &Html, options: Option<Options>) -> anyhow::Result<LyricsWithChords> {
        let user_options = options.unwrap_or_default();

        let parsed_data = Self::parse_data_from_dom(document)?;

        let tab_data = parsed_data
            .tab_view
            .replace("\r\n", "\n")
            .replace("[tab]", "")
            .replace("[/tab]", "");

        let nodes: Vec<TextNode> = match parse_lyrics_with_chords::<(&str, ErrorKind)>(&tab_data) {
            Ok(r) => r,
            Err((e, kind)) => return Err(Error::msg(format!("{}: {}", kind.description(), e))),
        }
        .iter()
        .map(|n| {
            if let TextNode::Chord(ch) = n {
                if ch.contains("B") {
                    if ch == "B" {
                        TextNode::Chord("H".to_string())
                    } else if ch == "Bb" {
                        TextNode::Chord("B".to_string())
                    } else if ch == "B#" {
                        TextNode::Chord("C".to_string())
                    } else {
                        unreachable!()
                    }
                } else {
                    n.clone()
                }
            } else {
                n.clone()
            }
        })
        .collect();

        let mut lines: Vec<Vec<TextNode>> = Vec::new();
        let mut line = Vec::new();
        for node in nodes {
            if node == TextNode::Newline {
                if line.len() != 0 {
                    lines.push(line);
                }
                line = Vec::new();
            } else {
                line.push(node);
            }
        }

        let mut merged_lines: Vec<Vec<TextNode>> = Vec::new();
        for (i, line) in lines.clone().iter().enumerate() {
            merged_lines.push(line.clone());

            if line.iter().any(|n| matches!(n, &TextNode::Label(_))) {
                if let TextNode::Label(l) = line[0].clone() {
                    // remove all but chorus labels,
                    // insert a newline in front of all chorus labels
                    if !l.to_lowercase().contains("chorus") {
                        merged_lines.pop();
                        merged_lines.push(vec![]);
                        continue;
                    } else {
                        merged_lines.pop();

                        merged_lines.push(vec![]);
                        merged_lines.push(vec![TextNode::Label(user_options.chorus_label.clone())]);
                    }
                }
            }

            if i == 0 {
                continue;
            }

            // ultimate guitar chords are formatted like this:
            // (chords and lyrics alternate line by line)
            // [Chords]
            // [Lyrics]
            // [Chords]
            // [Lyrics]
            // ...
            // here we merge the chords with lyrics into one line
            let previous_line = lines[i - 1].clone();
            let has_chord = line.iter().any(|n| matches!(n, &TextNode::Chord(_)));
            let previous_line_has_chord = previous_line
                .iter()
                .any(|n| matches!(n, &TextNode::Chord(_)) && !matches!(n, &TextNode::Label(_)));

            if has_chord {
                merged_lines.pop();

                // If there are only chords in this line, we remove the spaces between them
                // TODO: Detect streaks of chords instead to also delete spaces in lines with both text and chords
                merged_lines.push(
                    line.iter()
                        .filter(|n| matches!(*n, TextNode::Chord(_)))
                        .enumerate()
                        .flat_map(|(i, n)| {
                            if i != 0 {
                                vec![TextNode::Text(" ".to_string()), n.clone()]
                            } else {
                                vec![n.clone()]
                            }
                        })
                        .collect(),
                );
                continue;
            } else if !previous_line_has_chord {
                continue;
            }

            merged_lines.pop();
            merged_lines.pop();

            // Collect the text in this line
            let mut current_line_text_string = "".to_string();
            for node in line.iter() {
                if let TextNode::Text(t) = node {
                    current_line_text_string += t;
                }
            }

            // detect where the borders of words are
            // we do not want to put chords in the middle of words
            let mut index = 0;
            let possible_indices: Vec<usize> = current_line_text_string
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

            // we start with only text and split it into pieces
            // putting the chords in between
            let mut merged_line: Vec<TextNode> =
                vec![TextNode::Text(current_line_text_string.to_string())];

            let mut target_len = 0;
            for node in previous_line {
                match node {
                    TextNode::Text(ref k) => {
                        target_len += k.len();
                    }
                    TextNode::Chord(ref ch) => {
                        // we find where this chord should be put
                        let (_, closest_index) = possible_indices
                            .iter()
                            .map(|k| (target_len.abs_diff(*k), *k))
                            .sorted_by(|(a_diff, _ind1), (b_diff, _ind2)| Ord::cmp(a_diff, b_diff))
                            .nth(0)
                            .unwrap();

                        merged_line.push_chord(closest_index, TextNode::Chord(ch.clone()));

                        target_len += Self::CHORD_CHARACTER_WIDTH;
                    }
                    TextNode::Label(_) => unreachable!(),
                    TextNode::Newline => unreachable!(),
                }
            }

            // we delete spaces between streaks of chords
            merged_line = merged_line
                .iter()
                .enumerate()
                .flat_map(|(i, node)| {
                    if i == 0 {
                        return vec![node.clone()];
                    }

                    if matches!(merged_line[i - 1], TextNode::Chord(_))
                        && matches!(node, TextNode::Chord(_))
                    {
                        return vec![TextNode::Text(" ".to_string()), node.clone()];
                    }

                    vec![node.clone()]
                })
                .collect();

            merged_lines.push(merged_line);
        }

        Ok(LyricsWithChords::new(
            merged_lines.join(&TextNode::Newline),
            parsed_data.artist,
            parsed_data.song_name,
        ))
    }
}

pub fn string<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, &'a str, E> {
    let chars = "\n[]";

    take_while1(move |c| !chars.contains(c))(i)
}

pub fn text<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    map(string, |s: &str| TextNode::Text(s.to_string()))(i)
}

pub fn newline<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    let newline_take_while = take_while_m_n(1, 1, move |c| c == '\n');
    map(newline_take_while, |_| TextNode::Newline)(i)
}

pub fn chord<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "chord",
        map(
            preceded(tag("[ch]"), cut(terminated(string, tag("[/ch]")))),
            |o| TextNode::Chord(o.to_string()),
        ),
    )(i)
}

pub fn label<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "label",
        map(delimited(char('['), string, char(']')), |s| {
            TextNode::Label(s.to_string())
        }),
    )(i)
}

pub fn parse_lyrics_with_chords<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> Result<Vec<TextNode>, E> {
    let mut tag_parser = alt((
        chord::<'a, E>,
        label::<'a, E>,
        newline::<'a, E>,
        text::<'a, E>,
    ));

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

        tags.push(node);

        s = rest;
    }

    Ok(tags)
}
