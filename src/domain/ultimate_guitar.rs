use std::{collections::HashSet, io};

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
use scraper::{Html, Node, Selector};
use serde_json::Value;

use super::core::{Appendable, LyricsWithChords, Options, TextNode};

pub struct RawParsedData {
    pub artist: String,
    pub song_name: String,
    pub tab_view: String,
}

pub struct UltimateGuitar;

impl UltimateGuitar {
    const CHORD_CHARACTER_WIDTH: usize = 3;

    /// Ultimate Guitar embeds schema.org structured data for the song in a
    /// `<script type="application/ld+json">` tag; that's a much more stable
    /// source for the artist/title than the (hashed, build-specific) CSS
    /// classes the rest of the page uses.
    fn parse_song_info(document: &Html) -> anyhow::Result<(String, String)> {
        let script_selector = Selector::parse(r#"script[type="application/ld+json"]"#).map_err(
            |_| io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!"),
        )?;

        for element in document.select(&script_selector) {
            let text: String = element.text().collect();
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };

            let artist = value
                .get("byArtist")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str());
            let song_name = value.get("name").and_then(|n| n.as_str());

            if let (Some(artist), Some(song_name)) = (artist, song_name) {
                return Ok((artist.to_string(), song_name.to_string()));
            }
        }

        Err(Error::msg("Unexpected document structure! (song info)"))
    }

    /// Walks the chord sheet `<pre>` element, turning `<span data-name="...">`
    /// chord markup back into the `[ch]X[/ch]` bracket tags the rest of this
    /// module already knows how to parse. Non-span elements (Ultimate Guitar
    /// injects ad/marker elements like a trailing `<div>` directly inside the
    /// `<pre>`) are skipped since they aren't part of the tab.
    fn extract_tab_markup(node: ego_tree::NodeRef<Node>) -> String {
        let mut out = String::new();

        for child in node.children() {
            match child.value() {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) if e.name() == "span" => {
                    if let Some(chord_name) = e.attr("data-name") {
                        out.push_str("[ch]");
                        out.push_str(chord_name);
                        out.push_str("[/ch]");
                    } else {
                        out.push_str(&Self::extract_tab_markup(child));
                    }
                }
                _ => {}
            }
        }

        out
    }

    fn parse_data_from_dom(document: &Html) -> anyhow::Result<RawParsedData> {
        let (artist, song_name) = Self::parse_song_info(document)?;

        let pre_selector = Selector::parse("pre").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!")
        })?;

        let pre_element = document
            .select(&pre_selector)
            .nth(0)
            .context("Unexpected document structure! (pre)")?;

        let raw_markup = Self::extract_tab_markup(*pre_element);
        let known_chords = collect_known_chords(&raw_markup);
        let tab_view = tag_plain_chord_lines(&raw_markup, &known_chords);

        Ok(RawParsedData {
            artist,
            song_name,
            tab_view,
        })
    }

    pub fn get(document: &Html, options: Option<Options>) -> anyhow::Result<LyricsWithChords> {
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
                    if ch.starts_with("Bb") {
                        TextNode::Chord(ch.replace("Bb", "B").to_string())
                    } else if ch.starts_with("B#") {
                        TextNode::Chord(ch.replace("B#", "C").to_string())
                    } else if ch.starts_with("B") {
                        TextNode::Chord(ch.replace("B", "H").to_string())
                    } else {
                        TextNode::Chord(ch.replace("B", "H").to_string())
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

                        target_len += Self::CHORD_CHARACTER_WIDTH.max(ch.len());
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

/// Ultimate Guitar prints a chord-diagram legend above the tab body (e.g.
/// "G     3-5-5-4-3-3": a chord name followed by a dash-separated fret
/// pattern). It lists every chord used in the song, so we use it to
/// recognize chords that aren't wrapped in `<span data-name>` markup.
fn legend_chord_name(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let (name, rest) = trimmed.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();

    if name.is_empty() {
        return None;
    }

    let frets: Vec<&str> = rest.split('-').collect();
    let looks_like_fret_diagram = frets.len() >= 3
        && frets.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, 'x' | 'X' | 'o' | 'O'))
        });

    looks_like_fret_diagram.then(|| name.to_string())
}

fn collect_known_chords(text: &str) -> HashSet<String> {
    text.lines().filter_map(legend_chord_name).collect()
}

fn looks_like_chord(token: &str, known_chords: &HashSet<String>) -> bool {
    if token.is_empty() {
        return false;
    }

    if known_chords.contains(token) {
        return true;
    }

    let starts_with_note = matches!(token.chars().next(), Some('A'..='H'));

    starts_with_note
        && token.len() <= 8
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | 'b' | '/' | '+'))
}

/// Rewrites every maximal run of non-whitespace characters in `text`,
/// preserving the original whitespace layout (needed to keep chords aligned
/// over the right syllable once merged with the lyric line below).
fn wrap_words(text: &str, wrap: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut word_start: Option<usize> = None;

    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(start) = word_start.take() {
                out.push_str(&wrap(&text[start..i]));
            }
            out.push(c);
        } else if word_start.is_none() {
            word_start = Some(i);
        }
    }

    if let Some(start) = word_start {
        out.push_str(&wrap(&text[start..]));
    }

    out
}

/// Ultimate Guitar only wraps *some* chord occurrences in interactive
/// `<span data-name>` markup; repeats later in the tab are often left as
/// plain text. For every line that isn't a chord-diagram legend, this checks
/// whether *every* token on the line (already-tagged `[ch]...[/ch]` spans
/// count automatically) looks like a chord, and if so, wraps the remaining
/// plain-text tokens in `[ch]...[/ch]` too. Lines that mix real lyrics with
/// chord-like words are left completely untouched.
fn tag_plain_chord_lines(text: &str, known_chords: &HashSet<String>) -> String {
    text.lines()
        .map(|line| {
            if legend_chord_name(line).is_some() {
                // Drop chord-diagram legend lines entirely; they aren't lyrics.
                return String::new();
            }

            if line.trim().is_empty() {
                return line.to_string();
            }

            // Split out already-tagged `[ch]...[/ch]` spans so only the
            // plain-text parts need to be classified.
            let mut segments: Vec<(&str, bool)> = Vec::new();
            let mut rest = line;
            while let Some(start) = rest.find("[ch]") {
                if start > 0 {
                    segments.push((&rest[..start], false));
                }

                match rest[start..].find("[/ch]") {
                    Some(end) => {
                        let end = start + end + "[/ch]".len();
                        segments.push((&rest[start..end], true));
                        rest = &rest[end..];
                    }
                    None => {
                        segments.push((&rest[start..], false));
                        rest = "";
                        break;
                    }
                }
            }
            if !rest.is_empty() {
                segments.push((rest, false));
            }

            let all_chord_shaped = segments.iter().all(|(segment, is_tag)| {
                *is_tag || segment.split_whitespace().all(|tok| looks_like_chord(tok, known_chords))
            });

            if !all_chord_shaped {
                return line.to_string();
            }

            segments
                .into_iter()
                .map(|(segment, is_tag)| {
                    if is_tag {
                        segment.to_string()
                    } else {
                        wrap_words(segment, |word| format!("[ch]{word}[/ch]"))
                    }
                })
                .collect()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn string<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, &'a str, E> {
    let chars = "\n[]";

    take_while1(move |c| !chars.contains(c))(i)
}

fn text<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    map(string, |s: &str| TextNode::Text(s.to_string()))(i)
}

fn newline<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    let newline_take_while = take_while_m_n(1, 1, move |c| c == '\n');
    map(newline_take_while, |_| TextNode::Newline)(i)
}

fn chord<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
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

fn label<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "label",
        map(delimited(char('['), string, char(']')), |s| {
            TextNode::Label(s.to_string())
        }),
    )(i)
}

fn parse_lyrics_with_chords<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
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
