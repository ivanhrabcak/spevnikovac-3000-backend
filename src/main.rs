use domain::{core::LyricsWithChords, supermusic::Supermusic, ultimate_guitar::UltimateGuitar};
use export::{get_editing_hints, write_docx};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    // let url = "https://supermusic.cz/skupina.php?idpiesne=198707&sid=";
    // let url = "https://supermusic.cz/skupina.php?idpiesne=4523&sid=";
    let urls = [
        "https://supermusic.cz/skupina.php?idpiesne=1305&sid",
        "https://supermusic.cz/skupina.php?idpiesne=692279&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=4523&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=622&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=2132&sid=",
        "https://tabs.ultimate-guitar.com/tab/3718745",
        "https://supermusic.cz/skupina.php?idpiesne=329842&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=957655&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=316&sid",
        "https://supermusic.cz/skupina.php?idpiesne=48895&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=2669&sid",
        "https://supermusic.cz/skupina.php?idpiesne=1013&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=81835&sid=",
        "https://supermusic.cz/skupina.php?idpiesne=1706&sid=",
        "https://tabs.ultimate-guitar.com/tab/billy-joel/vienna-chords-804911",
        "https://tabs.ultimate-guitar.com/tab/radiohead/creep-chords-4169",
        "https://tabs.ultimate-guitar.com/tab/franz-ferdinand/take-me-out-chords-176648",
        "https://tabs.ultimate-guitar.com/tab/bill-withers/lovely-day-chords-1135417",
        "https://tabs.ultimate-guitar.com/tab/edward-sharpe-and-the-magnetic-zeros/home-chords-1091099",
        "https://tabs.ultimate-guitar.com/tab/iggy-pop/the-passenger-chords-80079",
        "https://tabs.ultimate-guitar.com/tab/goo-goo-dolls/iris-chords-54512",
        "https://tabs.ultimate-guitar.com/tab/frank-sinatra/fly-me-to-the-moon-chords-335196",
        "https://tabs.ultimate-guitar.com/tab/the-police/every-breath-you-take-chords-1087239",
        "https://tabs.ultimate-guitar.com/tab/red-hot-chili-peppers/otherside-chords-983702",
        "https://tabs.ultimate-guitar.com/tab/the-doors/people-are-strange-chords-1873736",
        "https://tabs.ultimate-guitar.com/tab/ed-sheeran/perfect-chords-1956589",
        "https://tabs.ultimate-guitar.com/tab/the-beatles/hey-jude-chords-1061739",
        "https://tabs.ultimate-guitar.com/tab/eagles/hotel-california-chords-46190"
    ];

    let mut all_lyrics = vec![];

    for url in urls {
        println!("Trying: {url}");
        // let lyrics = Supermusic::get(&document, &Html::parse_document(&text_1)).unwrap();

        let lyrics: LyricsWithChords = if url.contains("supermusic") {
            Supermusic::fetch_whole(url.to_string()).await.unwrap()
        } else {
            let client = reqwest::Client::new();

            let text = client.get(url).send().await.unwrap().text().await.unwrap();

            let document = Html::parse_document(&text);
            UltimateGuitar::get(&document, None).unwrap()
        };

        all_lyrics.push(lyrics);

        // write_docx(vec![lyrics], "songs.docx".to_string()).unwrap();
    }

    println!("Done!");
}
