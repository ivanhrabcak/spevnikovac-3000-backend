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
        let artist_selector = Selector::parse(".info-bar-artist").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!")
        })?;
        let song_name_selector = Selector::parse(".info-bar-song").map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to create selector!")
        })?;

        let artist = document
            .select(&artist_selector)
            .nth(0)
            .context("Unexpected document structure! (artist)")?
            .text()
            .collect::<String>()
            .trim()
            .to_string();

        let song_name = document
            .select(&song_name_selector)
            .nth(0)
            .context("Unexpected document structure! (song name)")?
            .text()
            .collect::<String>()
            .trim()
            .to_string();

        println!("Parsed artist and song: {artist}: {song_name}");

        let lf_template = txt_export_document.replace("\r\n", "\n");
        let mut song_template: Vec<&str> = lf_template.split("\n").collect();

        // The exported .txt file is prefixed with a variable number of metadata
        // lines (song title, and sometimes a duplicate "artist- title" line)
        // before the lyrics/chords start. Strip them by matching against the
        // artist/title we already parsed from the page, since the exact count
        // of header lines differs between older and newer catalogue entries.
        while let Some(first) = song_template.first() {
            let trimmed = first.trim();
            let is_metadata_line = trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case(&song_name)
                || trimmed.eq_ignore_ascii_case(&format!("{artist}- {song_name}"))
                || trimmed.eq_ignore_ascii_case(&format!("{artist} - {song_name}"));

            if !is_metadata_line {
                break;
            }

            song_template.remove(0);
        }

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
            println!("{:?}", line);
            let get_text = |n| {
                if let TextNode::Text(t) = n {
                    t
                } else if let TextNode::Chord(t) = n {
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
            "https://www.supermusic.cz/export.php?idpiesne={}&stiahni=1&typ=TXT&sid=",
            song_id
        );

        // The bare `supermusic.cz` domain 301-redirects to `www.supermusic.cz`.
        // We resolve that ourselves (rather than letting reqwest follow it) because
        // the anti-bot cookie we attach below would otherwise be dropped by
        // reqwest across that cross-host hop.
        let main_url = ensure_www_host(&url);

        let client = Client::new();
        let text_export_response = fetch_bypassing_bot_check(&client, &text_export_url).await?;
        let main_document = fetch_bypassing_bot_check(&client, &main_url).await?;

        Self::get(&Html::parse_document(&main_document), text_export_response)
    }
}

/// supermusic.cz shows visitors without a prior verified session a lightweight
/// interstitial page: it sets a `_sm_verified` cookie via a client-side script
/// and then reloads the original URL. Since we don't run JS, we read that
/// cookie value straight out of the interstitial's markup and replay it as a
/// request header on a second attempt.
async fn fetch_bypassing_bot_check(client: &Client, url: &str) -> anyhow::Result<String> {
    let text = client.get(url).send().await?.text().await?;

    let Some(cookie_value) = extract_verification_cookie(&text) else {
        return Ok(text);
    };

    Ok(client
        .get(url)
        .header(
            reqwest::header::COOKIE,
            format!("_sm_verified={cookie_value}"),
        )
        .send()
        .await?
        .text()
        .await?)
}

fn ensure_www_host(url: &str) -> String {
    if url.contains("://www.supermusic.cz") {
        url.to_string()
    } else {
        url.replacen("supermusic.cz", "www.supermusic.cz", 1)
    }
}

fn extract_verification_cookie(html: &str) -> Option<String> {
    let marker = "_sm_verified=";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find(';')? + start;

    Some(html[start..end].to_string())
}

fn string<'a, const ALLOW_NEWLINE: bool, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, &'a str, E> {
    let chars = if ALLOW_NEWLINE {
        "".to_string()
    } else {
        "\n".to_string()
    } + "[]";

    take_while1(move |c| !chars.contains(c))(i)
}

fn chord_block<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "chord",
        map(delimited(char('['), string::<true, E>, char(']')), |o| {
            TextNode::Chord(o.to_string())
        }),
    )(i)
}

fn text<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    i: &'a str,
) -> IResult<&'a str, TextNode, E> {
    context(
        "text",
        map(string::<false, E>, |o| TextNode::Text(o.to_string())),
    )(i)
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
