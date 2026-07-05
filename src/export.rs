use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
    time::Duration,
};

use anyhow::Context;
use docx::{
    document::{BodyContent, Break, BreakType, Paragraph, Run},
    Docx,
};
use itertools::Itertools;
use scraper::Html;
use serde::{Deserialize, Serialize};
use tauri::{scope::ipc::RemoteDomainAccessScope, AppHandle, Manager, WindowBuilder, WindowUrl};
use tokio::sync::oneshot;

use crate::domain::{
    core::{LyricsWithChords, TextNode},
    supermusic::Supermusic,
    ultimate_guitar::UltimateGuitar,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EditingHint {
    Node(TextNode),
    PossibleChordPlace,
}

#[tauri::command(async)]
pub async fn fetch(url: String, app: AppHandle) -> Result<LyricsWithChords, String> {
    if url.contains("ultimate-guitar.com") {
        let html = fetch_ultimate_guitar_html(&app, &url)
            .await
            .map_err(|e| e.to_string())?;

        let document = Html::parse_document(&html);

        UltimateGuitar::get(&document, None).map_err(|e| e.to_string())
    } else if url.contains("supermusic.cz") {
        Supermusic::fetch_whole(url)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("This source is not supported!".to_string())
    }
}

// Ultimate Guitar now sits behind a Cloudflare bot challenge that a plain HTTP
// client can never pass (it requires running the page's own JS, and
// sometimes an actual user click). Instead, we load the tab page in a real
// (visible) webview window and let it resolve the challenge like a normal
// browser tab would, then pull the rendered HTML back out once the real page
// has loaded.
static PENDING_UG_FETCHES: LazyLock<Mutex<HashMap<String, oneshot::Sender<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static NEXT_FETCH_ID: AtomicU64 = AtomicU64::new(0);

fn next_fetch_label() -> String {
    format!("ug-fetch-{}", NEXT_FETCH_ID.fetch_add(1, Ordering::Relaxed))
}

/// Called from JS injected into the fetch window once the real tab page (not
/// the Cloudflare interstitial) has finished loading.
#[tauri::command]
pub fn report_ug_page(label: String, html: String) {
    if let Some(sender) = PENDING_UG_FETCHES.lock().unwrap().remove(&label) {
        let _ = sender.send(html);
    }
}

async fn fetch_ultimate_guitar_html(app: &AppHandle, url: &str) -> anyhow::Result<String> {
    let parsed_url: tauri::Url = url.parse().context("Invalid URL")?;
    let domain = parsed_url
        .domain()
        .context("URL has no domain")?
        .to_string();

    let label = next_fetch_label();

    // Remote pages don't get access to the Tauri IPC bridge by default; grant
    // it just for this one throwaway window/domain pair so our injected
    // script can call `report_ug_page` back.
    app.ipc_scope().configure_remote_access(
        RemoteDomainAccessScope::new(domain)
            .add_window(&label)
            .enable_tauri_api(),
    );

    let (tx, rx) = oneshot::channel();
    PENDING_UG_FETCHES.lock().unwrap().insert(label.clone(), tx);

    let init_script = format!(
        r#"(function() {{
            var LABEL = {label};
            var reported = false;
            function trySend(force) {{
                if (reported) return;
                var ready = document.readyState === 'complete';
                var hasContent = !!document.querySelector('pre');
                if (ready && (hasContent || force)) {{
                    reported = true;
                    window.__TAURI_INVOKE__('report_ug_page', {{ label: LABEL, html: document.documentElement.outerHTML }});
                }}
            }}
            document.addEventListener('readystatechange', function() {{ trySend(false); }});
            var interval = setInterval(function() {{
                trySend(false);
                if (reported) clearInterval(interval);
            }}, 300);
            setTimeout(function() {{
                clearInterval(interval);
                trySend(true);
            }}, 20000);
        }})();"#,
        label = serde_json::to_string(&label).unwrap(),
    );

    let window = WindowBuilder::new(app, &label, WindowUrl::External(parsed_url))
        .title("Načítavam akordy...")
        .inner_size(480.0, 720.0)
        .visible(false)
        .initialization_script(&init_script)
        .build()
        .context("Failed to open fetch window")?;

    let result = tokio::time::timeout(Duration::from_secs(25), rx).await;

    let _ = window.close();
    PENDING_UG_FETCHES.lock().unwrap().remove(&label);

    match result {
        Ok(Ok(html)) => Ok(html),
        Ok(Err(_)) => Err(anyhow::Error::msg(
            "Fetch window closed before the page finished loading",
        )),
        Err(_) => Err(anyhow::Error::msg(
            "Timed out waiting for the Ultimate Guitar page to load",
        )),
    }
}

#[tauri::command]
pub fn get_editing_hints(nodes: Vec<TextNode>) -> Vec<EditingHint> {
    nodes
        .iter()
        .flat_map(|node| match node {
            TextNode::Text(t) => {
                let parts: Vec<&str> = t.split(" ").collect();

                parts
                    .iter()
                    .enumerate()
                    .flat_map(|(i, part)| {
                        if part.trim() == "" {
                            return vec![EditingHint::Node(TextNode::Text(" ".to_string()))];
                        }

                        let mut p = vec![
                            EditingHint::PossibleChordPlace,
                            EditingHint::Node(TextNode::Text(part.to_string())),
                        ];

                        if i != parts.len() - 1 {
                            p.append(&mut vec![
                                EditingHint::PossibleChordPlace,
                                EditingHint::Node(TextNode::Text(" ".to_string())),
                            ]);
                        }

                        p.push(EditingHint::PossibleChordPlace);

                        p
                    })
                    .collect::<Vec<EditingHint>>()
            }
            TextNode::Chord(_) => vec![
                EditingHint::PossibleChordPlace,
                EditingHint::Node(node.clone()),
                EditingHint::PossibleChordPlace,
            ],
            TextNode::Label(_) => vec![EditingHint::Node(node.clone())],
            TextNode::Newline => vec![EditingHint::Node(node.clone())],
        })
        .dedup_by(|a, b| {
            let is_text_node = |n| matches!(n, &EditingHint::Node(TextNode::Text(_)));
            let is_dedup_node = |n| matches!(n, &EditingHint::PossibleChordPlace);
            let extract_text = |n: &EditingHint| {
                if let EditingHint::Node(TextNode::Text(t)) = n.clone() {
                    t
                } else {
                    unreachable!()
                }
            };

            if !is_text_node(a) || !is_text_node(b) {
                is_dedup_node(a) && is_dedup_node(b)
            } else {
                extract_text(a) == extract_text(b)
            }
        })
        .collect()
}

#[tauri::command]
pub fn write_docx(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String> {
    let mut whole_document = Docx::default();

    for (song_i, song) in songs.iter().enumerate() {
        let song_paragraphs = song.clone().render_docx();
        for (i, paragraph) in song_paragraphs.iter().enumerate() {
            if song_i != 0 && i == 0 {
                let mut p = paragraph.clone();

                // TODO: Make page breaks work
                p.content.insert(
                    0,
                    docx::document::ParagraphContent::Run(
                        Run::default().push_break(Break::from(BreakType::Page)),
                    ),
                );

                whole_document.document.push(p);
            } else {
                whole_document.document.push(paragraph.clone());
            }
        }
    }

    whole_document
        .write_file(path)
        .map_err(|e| match e {
            docx::DocxError::IO(e) => e.to_string(),
            docx::DocxError::Xml(_e) => "Xml Error!".to_string(),
            docx::DocxError::Zip(e) => e.to_string(),
        })
        .map(|_| ())
}

#[tauri::command]
pub fn transpose(nodes: Vec<TextNode>, modifier: i32) -> Vec<TextNode> {
    let mut dummy_lyrics = LyricsWithChords::new(nodes, "".to_string(), "".to_string());

    dummy_lyrics.transpose(modifier);

    return dummy_lyrics.text;
}
