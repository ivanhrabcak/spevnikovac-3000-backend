use domain::{core::LyricsWithChords, supermusic::Supermusic, ultimate_guitar::UltimateGuitar};
use export::{get_editing_hints, write_chordpro, write_docx};
use scraper::Html;

pub mod domain;
pub mod export;

#[tokio::main]
async fn main() {
    // let url = "https://supermusic.cz/skupina.php?idpiesne=198707&sid=";
    // let url = "https://supermusic.cz/skupina.php?idpiesne=4523&sid=";
    let urls = [
        "https://tabs.ultimate-guitar.com/tab/bill-withers/aint-no-sunshine-chords-468744",
        "https://tabs.ultimate-guitar.com/tab/the-animals/house-of-the-rising-sun-chords-18688",
        "https://tabs.ultimate-guitar.com/tab/misc-soundtrack/a-star-is-born-shallow-chords-2488086",
        "https://tabs.ultimate-guitar.com/tab/hozier/take-me-to-church-chords-1442757",
        "https://tabs.ultimate-guitar.com/tab/avicii/hey-brother-chords-1426261",
        "https://tabs.ultimate-guitar.com/tab/eric-clapton/tears-in-heaven-chords-627220",
        "https://tabs.ultimate-guitar.com/tab/johnny-cash/ring-of-fire-chords-63352",
        "https://tabs.ultimate-guitar.com/tab/billie-eilish/birds-of-a-feather-chords-5270913",
        "https://tabs.ultimate-guitar.com/tab/frank-sinatra/somethin-stupid-chords-831238",
        "https://tabs.ultimate-guitar.com/tab/carly-rae-jepsen/call-me-maybe-chords-1120096",
        "https://tabs.ultimate-guitar.com/tab/oasis/wonderwall-chords-27596",
        "https://tabs.ultimate-guitar.com/tab/misc-cartoons/spongebob-squarepants-beginning-theme-chords-1937167",
        "https://tabs.ultimate-guitar.com/tab/chappell-roan/pink-pony-club-chords-3066623",
        "https://tabs.ultimate-guitar.com/tab/fleetwood-mac/the-chain-chords-1054619",
        "https://tabs.ultimate-guitar.com/tab/billy-joel/piano-man-chords-1051336",
        "https://tabs.ultimate-guitar.com/tab/onerepublic/counting-stars-chords-1233464",
        "https://tabs.ultimate-guitar.com/tab/walk-the-moon/shut-up-and-dance-chords-1673689",
        "https://tabs.ultimate-guitar.com/tab/bonnie-tyler/holding-out-for-a-hero-chords-430301",
        "https://tabs.ultimate-guitar.com/tab/don-mclean/american-pie-chords-187946",
        "https://tabs.ultimate-guitar.com/tab/tones-and-i/dance-monkey-chords-2787730",
        "https://tabs.ultimate-guitar.com/tab/misc-cartoons/frozen-let-it-go-chords-1445224",
        "https://tabs.ultimate-guitar.com/tab/3946514",
        "https://tabs.ultimate-guitar.com/tab/2728767",
        "https://tabs.ultimate-guitar.com/tab/3787520",
        "https://tabs.ultimate-guitar.com/tab/3135836",
        "https://tabs.ultimate-guitar.com/tab/1483105",
        "https://tabs.ultimate-guitar.com/tab/karel-gott/trezor-chords-3214292",
        "https://tabs.ultimate-guitar.com/tab/4393100",
        "https://tabs.ultimate-guitar.com/tab/tublatanka/dnes-chords-1675825",
        "https://tabs.ultimate-guitar.com/tab/3276479",
        "https://tabs.ultimate-guitar.com/tab/2961230",
        "https://tabs.ultimate-guitar.com/tab/3711731",
        "https://tabs.ultimate-guitar.com/tab/jana-kirschner/v-cudzom-meste-chords-3414083",
        "https://tabs.ultimate-guitar.com/tab/2885879",
        "https://tabs.ultimate-guitar.com/tab/petr-novak/ja-budu-chodit-po-spickach-chords-1426802",
        "https://tabs.ultimate-guitar.com/tab/2887844",
        "https://tabs.ultimate-guitar.com/tab/2900381",
        "https://tabs.ultimate-guitar.com/tab/2435117",
        "https://tabs.ultimate-guitar.com/tab/2340123",
        "https://tabs.ultimate-guitar.com/tab/2394387",
        "https://tabs.ultimate-guitar.com/tab/michal-david/nonstop-chords-4246534",
        "https://tabs.ultimate-guitar.com/tab/michal-david/nonstop-chords-4246534",
        "https://tabs.ultimate-guitar.com/tab/3093542",
        "https://tabs.ultimate-guitar.com/tab/2464930",
        "https://tabs.ultimate-guitar.com/tab/hana-hegerova/levandulova-chords-1452470",
        "https://www.supermusic.cz/piesen.php?idpiesne=997232"
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

    if let Err(e) = write_chordpro(all_lyrics, "songs.cho".to_string()) {
        eprintln!("Failed to write ChordPro output: {e}");
    }

    println!("Done!");
}
