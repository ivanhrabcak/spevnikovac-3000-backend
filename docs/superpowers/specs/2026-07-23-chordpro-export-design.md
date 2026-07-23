# ChordPro export — design

> **Superseded in part:** `write_chordpro` is now a `#[tauri::command]` writing one file per song into a directory (`dir: String` parameter), not a plain fn writing a single bundled file (`path: String`) as described below. See [2026-07-23-frontend-integration-design.md](2026-07-23-frontend-integration-design.md) for the current interface and rationale. Kept here as the historical record of the original design.

## Goal

Support exporting `LyricsWithChords` songs as [ChordPro](https://www.chordpro.org/chordpro/home/) text, alongside the existing docx export. For now this is wired up only through a hardcoded song list in `main.rs` — no Tauri command, no frontend integration.

## Background

`LyricsWithChords` (`src/domain/core.rs`) holds:

```rust
pub struct LyricsWithChords {
    pub text: Vec<TextNode>,
    pub artist: String,
    pub song_name: String,
}

pub enum TextNode {
    Text(String),
    Chord(String),
    Label(String),
    Newline,
}
```

Chords are already positioned inline within the text stream (e.g. `Text("Amaz")`, `Chord("G")`, `Text("ing grace")`), which lines up almost exactly with ChordPro's own inline chord syntax (`[G]Amazing`).

`Label` is only ever produced for a chorus marker (see `ultimate_guitar.rs`): a fixed string (default `®:`) placed as the sole node of its own line, always preceded by a blank line. Both parsers (`supermusic.rs`, `ultimate_guitar.rs`) drop genuinely blank source lines when building `lines`, so a blank line surviving into the final `text` is only ever introduced at a stripped/converted section-label position — it reliably marks a section boundary.

Existing docx export (`render_docx` in `core.rs`, `write_docx` in `export.rs`) is the pattern to mirror: a per-song render function producing document content, plus a top-level writer that bundles multiple songs into one output.

## Design

### 1. `LyricsWithChords::render_chordpro(&self) -> String` (`src/domain/core.rs`)

Renders one song as ChordPro text.

- Emit metadata header:
  ```
  {title: <song_name>}
  {artist: <artist>}
  ```
- Split `self.text` into lines on `TextNode::Newline`, then render line by line:
  - `TextNode::Text(t)` → written verbatim.
  - `TextNode::Chord(ch)` → written as `[ch]` at its existing position in the line (no extra spacing logic needed — position in the node stream already matches ChordPro's inline convention).
  - `TextNode::Label(_)` → does not render its literal text. Instead, emits `{start_of_chorus}` on its own line and sets `in_chorus = true`. (The stored label text, e.g. `®:`, is redundant once we have a real directive, so it's dropped.)
  - An empty line (a line with zero nodes) encountered while `in_chorus` is true → emit `{end_of_chorus}` on its own line *before* the blank line, then clear `in_chorus`. Empty lines encountered while not `in_chorus` pass through unchanged (preserve spacing between sections).
- After processing all lines, if `in_chorus` is still true (song ends without a trailing blank line), emit a closing `{end_of_chorus}`.
- Returns the assembled `String` (lines joined with `\n`).

### 2. `write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String>` (`src/export.rs`)

Plain `pub fn` (not a `#[tauri::command]` — no frontend wiring for this feature yet).

- Calls `render_chordpro()` on each song.
- Joins the per-song strings with a `{new_song}` directive line in between (mirrors the page-break-between-songs behavior of `write_docx`).
- Writes the joined text to `path` via `std::fs::write`, mapping any IO error to `String` (consistent with `write_docx`'s error-to-`String` convention).

### 3. `src/main.rs`

- Keep the existing hardcoded URL list and fetch loop as-is.
- After the fetch loop, call `export::write_chordpro(all_lyrics, "songs.cho".to_string())`, matching the currently-commented-out `write_docx` call, and handle/print the result.

## Out of scope

- No Tauri command, no frontend changes.
- No general-purpose section-label support (verse/bridge/etc.) — only chorus, since that's all the current data model produces.
- No per-song file output — single `.cho` file with `{new_song}` separators, matching the existing single-`path` docx signature.

## Testing

- Unit test(s) for `render_chordpro` covering: plain text+chord line, a chorus block closed by a blank line, and a chorus block that runs to the end of the song with no trailing blank line.
- Manual run of `main.rs` against the hardcoded URL list, inspecting the resulting `songs.cho` for correctness (title/artist headers, inline chord placement, chorus directives, `{new_song}` separators).
