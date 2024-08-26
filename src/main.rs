use domain::{core::Source, ultimate_guitar::UltimateGuitar};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    let url = "https://tabs.ultimate-guitar.com/tab/radiohead/just-chords-196011";
    let client = reqwest::Client::new();

    let text = client.get(url).send().await.unwrap().text().await.unwrap();

    let document = Html::parse_document(&text);
    let lyrics = UltimateGuitar::get(&document, None).unwrap();

    let mut doc = lyrics.render_docx();

    doc.write_file("just.docx").unwrap();
}
