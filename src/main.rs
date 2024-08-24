use core::str;
use std::{
    any::Any,
    collections::HashMap,
    io::{Cursor, Error, Read, Write},
};

use anyhow::Result;
use docx::{
    document::{Paragraph, Run},
    formatting::{CharacterProperty, ParagraphProperty, VerticalAlignment, VerticalAlignmentType},
    Docx,
};
use domain::ultimate_guitar::{parse_lyrics_with_chords, text, UltimateGuitar};
use nom::{bytes::complete::take_while, character::is_alphabetic, error::ErrorKind, IResult};

use reqwest::Client;
use scraper::{selectable::Selectable, Html, Selector};
use serde_json::Value;
use tokio::io::AsyncSeekExt;
use xml::{writer::XmlEvent, EmitterConfig};

pub mod domain;

#[tokio::main]
async fn main() {
    let url = "https://tabs.ultimate-guitar.com/tab/radiohead/just-chords-196011";
    let client = reqwest::Client::new();

    let text = client.get(url).send().await.unwrap().text().await.unwrap();

    let document = Html::parse_document(&text);
    let lyrics = UltimateGuitar::get(&document).unwrap();

    let mut doc = lyrics.render();

    doc.write_file("just.docx").unwrap();
}
