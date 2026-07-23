# ChordPro Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ChordPro text export for `LyricsWithChords` songs, wired up only through a hardcoded song list in `main.rs` (no Tauri command, no frontend).

**Architecture:** A per-song renderer (`LyricsWithChords::render_chordpro`) mirrors the existing `render_docx` pattern in `src/domain/core.rs`, producing a ChordPro-formatted `String`. A plain (non-`#[tauri::command]`) writer function `write_chordpro` in `src/export.rs` mirrors `write_docx`, bundling multiple songs into one `.cho` file separated by `{new_song}` directives. `main.rs` calls it after its existing fetch loop.

**Tech Stack:** Rust, no new dependencies. Uses `std::fs::write` for file output and plain `#[test]` unit tests (no test framework currently in the project; none needed).

## Global Constraints

- No `#[tauri::command]` annotation on any new function — this feature is not exposed to the frontend yet (per design doc).
- No new crate dependencies.
- ~~Chorus detection: any line containing a `TextNode::Label` starts a chorus (`{start_of_chorus}`); the label's literal text is not rendered. A blank line (a line with zero nodes) while inside a chorus closes it (`{end_of_chorus}`); if the song ends while still inside a chorus, close it after the last line.~~ Superseded: this inference didn't work reliably in practice and was removed. `TextNode::Label` now renders as its literal text on its own line (e.g. `®:`), same as `render_docx` already did — no directives, no chorus state tracking.
- ~~Single output file per `write_chordpro` call...~~ Superseded by Task 4: `write_chordpro` now writes one `.cho` file per song into a directory, named `"{artist} - {song_name}.cho"` — this is the actual convention ChordPro tooling expects, not a single bundled file.

---

### Task 1: `LyricsWithChords::render_chordpro`

**Files:**
- Modify: `src/domain/core.rs` (add method to the existing `impl LyricsWithChords` block, after `render_docx`, which ends at `src/domain/core.rs:90`)
- Test: `src/domain/core.rs` (new `#[cfg(test)] mod tests` block at the end of the file)

**Interfaces:**
- Consumes: `LyricsWithChords { text: Vec<TextNode>, artist: String, song_name: String }` and `TextNode { Text(String), Chord(String), Label(String), Newline }`, both already defined in this file.
- Produces: `pub fn render_chordpro(&self) -> String` on `LyricsWithChords`, used by `write_chordpro` in Task 2.

- [ ] **Step 1: Write the failing tests**

Add this at the very end of `src/domain/core.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{LyricsWithChords, TextNode};

    #[test]
    fn render_chordpro_plain_line_with_inline_chord() {
        let song = LyricsWithChords::new(
            vec![
                TextNode::Text("Amaz".to_string()),
                TextNode::Chord("G".to_string()),
                TextNode::Text("ing grace".to_string()),
            ],
            "Traditional".to_string(),
            "Amazing Grace".to_string(),
        );

        let expected = "\
{title: Amazing Grace}
{artist: Traditional}
Amaz[G]ing grace";

        assert_eq!(song.render_chordpro(), expected);
    }

    #[test]
    fn render_chordpro_chorus_closed_by_blank_line() {
        let song = LyricsWithChords::new(
            vec![
                TextNode::Text("Verse line".to_string()),
                TextNode::Newline,
                TextNode::Newline,
                TextNode::Label("®:".to_string()),
                TextNode::Newline,
                TextNode::Text("Chorus line".to_string()),
                TextNode::Newline,
                TextNode::Newline,
                TextNode::Text("Outro line".to_string()),
            ],
            "Artist".to_string(),
            "Song".to_string(),
        );

        let expected = "\
{title: Song}
{artist: Artist}
Verse line

{start_of_chorus}
Chorus line
{end_of_chorus}

Outro line";

        assert_eq!(song.render_chordpro(), expected);
    }

    #[test]
    fn render_chordpro_chorus_open_at_end_of_song() {
        let song = LyricsWithChords::new(
            vec![
                TextNode::Label("®:".to_string()),
                TextNode::Newline,
                TextNode::Text("Last chorus line".to_string()),
            ],
            "Artist".to_string(),
            "Song".to_string(),
        );

        let expected = "\
{title: Song}
{artist: Artist}
{start_of_chorus}
Last chorus line
{end_of_chorus}";

        assert_eq!(song.render_chordpro(), expected);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test render_chordpro`
Expected: compile error — `no method named 'render_chordpro' found for struct 'LyricsWithChords'`

- [ ] **Step 3: Implement `render_chordpro`**

Add this method inside the existing `impl LyricsWithChords { ... }` block in `src/domain/core.rs`, directly after the closing brace of `render_docx` (`src/domain/core.rs:90`):

```rust
    pub fn render_chordpro(&self) -> String {
        let mut lines: Vec<Vec<TextNode>> = Vec::new();
        let mut current_line: Vec<TextNode> = Vec::new();

        for node in self.text.iter() {
            if matches!(node, TextNode::Newline) {
                lines.push(current_line);
                current_line = Vec::new();
            } else {
                current_line.push(node.clone());
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        let mut output_lines: Vec<String> =
            vec![format!("{{title: {}}}", self.song_name), format!("{{artist: {}}}", self.artist)];

        let mut in_chorus = false;

        for line in lines {
            if line.is_empty() {
                if in_chorus {
                    output_lines.push("{end_of_chorus}".to_string());
                    in_chorus = false;
                }

                output_lines.push(String::new());
                continue;
            }

            if line.iter().any(|n| matches!(n, TextNode::Label(_))) {
                output_lines.push("{start_of_chorus}".to_string());
                in_chorus = true;
                continue;
            }

            let mut rendered_line = String::new();
            for node in line {
                match node {
                    TextNode::Text(t) => rendered_line.push_str(&t),
                    TextNode::Chord(ch) => {
                        rendered_line.push('[');
                        rendered_line.push_str(&ch);
                        rendered_line.push(']');
                    }
                    TextNode::Label(_) | TextNode::Newline => unreachable!(),
                }
            }

            output_lines.push(rendered_line);
        }

        if in_chorus {
            output_lines.push("{end_of_chorus}".to_string());
        }

        output_lines.join("\n")
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test render_chordpro`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add src/domain/core.rs
git commit -m "Add ChordPro rendering for LyricsWithChords"
```

---

### Task 2: `write_chordpro`

**Files:**
- Modify: `src/export.rs` (add function after `write_docx`, which ends at `src/export.rs:242`)

**Interfaces:**
- Consumes: `LyricsWithChords::render_chordpro(&self) -> String` from Task 1.
- Produces: `pub fn write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String>`, used by `main.rs` in Task 3.

- [ ] **Step 1: Implement `write_chordpro`**

Add this function to `src/export.rs`, directly after the closing brace of `write_docx` (`src/export.rs:242`). It is a plain function, not a `#[tauri::command]`:

```rust
pub fn write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String> {
    let contents = songs
        .iter()
        .map(|song| song.clone().render_chordpro())
        .collect::<Vec<String>>()
        .join("\n{new_song}\n");

    std::fs::write(path, contents).map_err(|e| e.to_string())
}
```

`LyricsWithChords` is already imported in this file (`src/export.rs:22`, `crate::domain::core::{LyricsWithChords, TextNode}`), so no new import is needed.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: builds with no new errors (pre-existing unused-import warnings for `get_editing_hints`/`write_docx` in `main.rs` are expected and unrelated to this change).

- [ ] **Step 3: Commit**

```bash
git add src/export.rs
git commit -m "Add write_chordpro to bundle songs into a ChordPro file"
```

---

### Task 3: Wire up `main.rs`

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `export::write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String>` from Task 2.

- [ ] **Step 1: Import `write_chordpro` and call it after the fetch loop**

In `src/main.rs`, change the import line (`src/main.rs:2`):

```rust
use export::{get_editing_hints, write_chordpro, write_docx};
```

Then replace the end of the `main` function (`src/main.rs:60-66`):

```rust
        all_lyrics.push(lyrics);

        // write_docx(vec![lyrics], "songs.docx".to_string()).unwrap();
    }

    println!("Done!");
}
```

with:

```rust
        all_lyrics.push(lyrics);

        // write_docx(vec![lyrics], "songs.docx".to_string()).unwrap();
    }

    if let Err(e) = write_chordpro(all_lyrics, "songs.cho".to_string()) {
        eprintln!("Failed to write ChordPro output: {e}");
    }

    println!("Done!");
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: builds with no new errors.

- [ ] **Step 3: Manually run and inspect the output**

Run: `cargo run`
Expected: the program fetches all hardcoded songs (as it already does today) and, on completion, a `songs.cho` file appears in the project root.

Open `songs.cho` and confirm:
- Each song starts with `{title: ...}` and `{artist: ...}` lines.
- Chords appear inline as `[Chord]` immediately before the syllable/word they apply to.
- Any chorus section is wrapped in `{start_of_chorus}` / `{end_of_chorus}`.
- Songs are separated by a `{new_song}` line.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "Write ChordPro output for the hardcoded song list in main"
```

---

### Task 4: Per-song file output (revision)

**Context:** After Tasks 1-3 landed, the user reviewed ChordPro's conventions and asked for one `.cho` file per song instead of a single bundled file with `{new_song}` separators. This task revises `write_chordpro`'s output shape; `render_chordpro` (Task 1) is unchanged.

**Files:**
- Modify: `src/export.rs` (rewrite the body of `write_chordpro`)
- Modify: `src/main.rs` (change the `write_chordpro` call site: pass a directory instead of a file path)

**Interfaces:**
- Consumes: `LyricsWithChords::render_chordpro(&self) -> String` (Task 1, unchanged), `LyricsWithChords { artist: String, song_name: String, .. }`.
- Produces: `pub fn write_chordpro(songs: Vec<LyricsWithChords>, dir: String) -> Result<(), String>` — same name, changed second parameter's meaning (directory, not a single file path) and behavior (writes N files, not 1).

- [ ] **Step 1: Rewrite `write_chordpro`**

Replace the current body of `write_chordpro` in `src/export.rs` (currently a `.join("\n{new_song}\n")` into one file) with:

```rust
pub fn write_chordpro(songs: Vec<LyricsWithChords>, dir: String) -> Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    for song in songs.iter() {
        let file_name = sanitize_filename(&format!("{} - {}", song.artist, song.song_name));
        let path = std::path::Path::new(&dir).join(format!("{file_name}.cho"));

        std::fs::write(path, song.render_chordpro()).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c })
        .collect()
}
```

This also fixes the Task 2 reviewer's Minor note (`song.clone().render_chordpro()` → `song.render_chordpro()`, since it takes `&self`).

- [ ] **Step 2: Update the `main.rs` call site**

Change the `write_chordpro` call added in Task 3 from a file path to a directory path:

```rust
    if let Err(e) = write_chordpro(all_lyrics, "songs".to_string()) {
        eprintln!("Failed to write ChordPro output: {e}");
    }
```

(directory `songs/` instead of file `songs.cho`)

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: builds with no new errors.

- [ ] **Step 4: Manually verify**

Same approach as Task 3's Step 3 (the full 46-song network run is unreliable due to the pre-existing Ultimate Guitar Cloudflare-blocking issue — verify with a temporarily-shrunk URL list, reverted before commit): confirm a `songs/` directory appears containing one `.cho` file per song, named `"{artist} - {song_name}.cho"`, each with the same per-song content `render_chordpro()` already produces (verified by Task 1's unit tests).

- [ ] **Step 5: Commit**

```bash
git add src/export.rs src/main.rs
git commit -m "Write one ChordPro file per song instead of one bundled file"
```
