use domain::{
    core::{LyricsWithChords, Source},
    ultimate_guitar::UltimateGuitar,
};
use export::{get_editing_hints, write_docx};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    let url = "https://tabs.ultimate-guitar.com/tab/radiohead/just-chords-196011";
    let url_1 = "https://tabs.ultimate-guitar.com/tab/the-monkees/im-a-believer-chords-25298";

    let client = reqwest::Client::new();

    let text = client.get(url).send().await.unwrap().text().await.unwrap();
    let text_1 = client
        .get(url_1)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let document = Html::parse_document(&text);
    let document_1 = Html::parse_document(&text_1);

    let lyrics = UltimateGuitar::get(&document, None).unwrap();
    let lyrics_1: LyricsWithChords = UltimateGuitar::get(&document_1, None).unwrap();
    // let mut doc = lyrics.render_docx();

    // println!("{:?}", get_editing_hints(lyrics.text.clone()));

    write_docx(vec![lyrics, lyrics_1], "songs.docx".to_string()).unwrap();
}
