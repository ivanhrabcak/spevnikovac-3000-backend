use domain::{core::LyricsWithChords, supermusic::Supermusic, ultimate_guitar::UltimateGuitar};
use export::{get_editing_hints, write_docx};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    // let url = "https://supermusic.cz/skupina.php?idpiesne=198707&sid=";
    // let url = "https://supermusic.cz/skupina.php?idpiesne=4523&sid=";
    let url = "https://tabs.ultimate-guitar.com/tab/bill-withers/lovely-day-chords-1135417";

    let client = reqwest::Client::new();

    let text = client.get(url).send().await.unwrap().text().await.unwrap();
    let text_1 = client.get(url).send().await.unwrap().text().await.unwrap();

    let document = Html::parse_document(&text);

    // let lyrics = Supermusic::get(&document, &Html::parse_document(&text_1)).unwrap();

    // let lyrics = Supermusic::fetch_whole(url.to_string()).await.unwrap();
    let lyrics = UltimateGuitar::get(&document, None).unwrap();

    // let mut doc = lyrics.render_docx();

    // println!("{:?}", get_editing_hints(lyrics.text.clone()));

    write_docx(vec![lyrics], "songs.docx".to_string()).unwrap();
}
