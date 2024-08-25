use std::{cmp::min, collections::HashMap, io};

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

use super::core::{Appendable, LyricsWithChords, TextNode};

pub struct UltimateGuitar;

impl UltimateGuitar {
    const CHORD_CHARACTER_WIDTH: usize = 3;

    fn parse_data_from_dom(document: &Html) -> anyhow::Result<String> {
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
        let tab_view = parsed_content
            .get("store")
            .context("Unexpected DOM structure! (store)")?
            .get("page")
            .context("Unexpected DOM structure! (page)")?
            .get("data")
            .context("Unexpected DOM structure! (data)")?
            .get("tab_view")
            .context("Unexpected DOM structure! (tab_view)")?
            .get("wiki_tab")
            .context("Unexpected DOM structure! (wiki_tab)")?
            .get("content")
            .context("Unexpected DOM structure! (content)")?
            .as_str()
            .context("Unexpected content value type!")?;

        return Ok(tab_view.to_string());
    }

    pub fn get(document: &Html) -> anyhow::Result<LyricsWithChords> {
        let tab_data = Self::parse_data_from_dom(document)?
            .replace("\r\n", "\n")
            .replace("[tab]", "")
            .replace("[/tab]", "");

        let nodes = match parse_lyrics_with_chords::<(&str, ErrorKind)>(&tab_data) {
            Ok(r) => r,
            Err((e, kind)) => return Err(Error::msg(format!("{}: {}", kind.description(), e))),
        };

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
                    if !l.to_lowercase().contains("chorus") {
                        merged_lines.pop();
                        merged_lines.push(vec![]);
                        continue;
                    } else {
                        let item = merged_lines.pop().unwrap();
                        merged_lines.push(vec![]);
                        merged_lines.push(item);
                    }
                }
            }

            if i == 0 {
                continue;
            }

            let previous_line = lines[i - 1].clone();
            let has_chord = line.iter().any(|n| matches!(n, &TextNode::Chord(_)));
            let previous_line_has_chord = previous_line
                .iter()
                .any(|n| matches!(n, &TextNode::Chord(_)) && !matches!(n, &TextNode::Label(_)));

            if has_chord {
                merged_lines.pop();

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
            }
            if !previous_line_has_chord {
                continue;
            }

            merged_lines.pop();
            merged_lines.pop();

            let mut current_line_text_string = "".to_string();
            for node in line.iter() {
                if let TextNode::Text(t) = node {
                    current_line_text_string += t;
                }
            }

            let mut index = 0;
            let mut possible_indices: Vec<usize> = current_line_text_string
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

            let l = possible_indices.len();
            if possible_indices[l - 1] != 0 {
                // possible_indices[l - 1] -= 1;
            }

            let mut merged_line: Vec<TextNode> =
                vec![TextNode::Text(current_line_text_string.to_string())];
            let mut target_len = 0;
            for node in previous_line {
                match node {
                    TextNode::Text(ref k) => {
                        target_len += k.len();
                    }
                    TextNode::Chord(ref ch) => {
                        let (_, closest_index) = possible_indices
                            .iter()
                            .map(|k| (target_len.abs_diff(*k), *k))
                            .sorted_by(|(a_diff, _ind1), (b_diff, _ind2)| Ord::cmp(a_diff, b_diff))
                            .nth(0)
                            .unwrap();

                        merged_line.push_chord(closest_index, TextNode::Chord(ch.clone()));

                        // let mut current_index = 0;
                        // merged_line = merged_line
                        //     .iter()
                        //     .flat_map(|n| {
                        //         if let TextNode::Chord(_) = n {
                        //             vec![n.clone()]
                        //         } else if let TextNode::Text(t) = n {
                        //             // the chord belongs to this text block
                        //             if closest_index >= current_index
                        //                 && closest_index <= current_index + t.len()
                        //             {
                        //                 let start = t[0..closest_index - current_index].to_string();
                        //                 let end =
                        //                     t[closest_index - current_index..t.len()].to_string();

                        //                 current_index += t.len();

                        //                 vec![
                        //                     TextNode::Text(start),
                        //                     TextNode::Chord(ch.clone()),
                        //                     TextNode::Text(end),
                        //                 ]
                        //             } else {
                        //                 current_index += t.len();

                        //                 vec![TextNode::Text(t.clone())]
                        //             }
                        //         } else {
                        //             unreachable!();
                        //         }
                        //     })
                        //     .collect();

                        target_len += Self::CHORD_CHARACTER_WIDTH;
                    }
                    TextNode::Label(_) => unreachable!(),
                    TextNode::Newline => unreachable!(),
                }
            }

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

        Ok(LyricsWithChords::new(merged_lines.join(&TextNode::Newline)))
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
