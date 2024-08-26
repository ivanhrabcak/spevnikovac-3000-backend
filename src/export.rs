use scraper::Html;

use crate::domain::{
    core::{LyricsWithChords, Source},
    ultimate_guitar::UltimateGuitar,
};

#[tauri::command(async)]
pub async fn fetch(url: String) -> LyricsWithChords {
    let client = reqwest::Client::new();

    let text = client.get(url).send().await.unwrap().text().await.unwrap();

    let document = Html::parse_document(&text);
    UltimateGuitar::get(&document, None).unwrap()
}
