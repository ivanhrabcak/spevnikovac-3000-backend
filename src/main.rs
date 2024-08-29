use domain::{core::LyricsWithChords, supermusic::Supermusic, ultimate_guitar::UltimateGuitar};
use export::{get_editing_hints, write_docx};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    let url = "https://supermusic.cz/skupina.php?idpiesne=454926&sid=&TEXT=1";
    let txt_url = "https://supermusic.cz/export.php?idpiesne=454926&typ=TXT";

    // let client = reqwest::Client::new();

    // let text = client.get(url).send().await.unwrap().text().await.unwrap();
    // let text_1 = client
    //     .get(txt_url)
    //     .send()
    //     .await
    //     .unwrap()
    //     .text()
    //     .await
    //     .unwrap();

    // let document = Html::parse_document(&text);

    // let lyrics = Supermusic::get(&document, &Html::parse_document(&text_1)).unwrap();

    let lyrics = Supermusic::fetch_whole(url.to_string()).await.unwrap();

    // let mut doc = lyrics.render_docx();

    // println!("{:?}", get_editing_hints(lyrics.text.clone()));

    write_docx(vec![lyrics], "songs.docx".to_string()).unwrap();
}
