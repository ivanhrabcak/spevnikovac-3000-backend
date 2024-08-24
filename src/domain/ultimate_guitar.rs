use std::{cmp::min, collections::HashMap, io};

use anyhow::{Context, Error};
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

use super::core::{LyricsWithChords, TextNode};

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

            if i == 0 {
                continue;
            }

            if line.iter().any(|n| matches!(n, &TextNode::Label(_))) {
                continue;
            }

            let previous_line = lines[i - 1].clone();
            let has_chord = line.iter().any(|n| matches!(n, &TextNode::Chord(_)));
            let previous_line_has_chord = previous_line
                .iter()
                .any(|n| matches!(n, &TextNode::Chord(_)) && !matches!(n, &TextNode::Label(_)));

            if has_chord || !previous_line_has_chord {
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

            let mut merged_line: Vec<TextNode> = Vec::new();
            let mut target_len = 0;
            for node in previous_line {
                match node {
                    TextNode::Text(k) => {
                        target_len += k.len();

                        if target_len - k.len() <= current_line_text_string.len() {
                            merged_line.push(TextNode::Text(
                                current_line_text_string[target_len - k.len()
                                    ..min(target_len, current_line_text_string.len())]
                                    .to_string(),
                            ));
                        }
                    }
                    TextNode::Chord(ch) => {
                        merged_line.push(TextNode::Chord(ch));
                        target_len += Self::CHORD_CHARACTER_WIDTH;

                        if target_len - Self::CHORD_CHARACTER_WIDTH
                            <= current_line_text_string.len()
                        {
                            merged_line.push(TextNode::Text(
                                current_line_text_string[target_len - Self::CHORD_CHARACTER_WIDTH
                                    ..min(target_len, current_line_text_string.len())]
                                    .to_string(),
                            ))
                        }
                    }
                    TextNode::Label(_) => unreachable!(),
                    TextNode::Newline => unreachable!(),
                }
            }

            if target_len < current_line_text_string.len() {
                merged_line.push(TextNode::Text(
                    current_line_text_string[target_len..current_line_text_string.len()]
                        .to_string(),
                ));
            }

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
