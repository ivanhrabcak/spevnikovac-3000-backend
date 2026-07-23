# Frontend Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the real desktop app (`spevnikovac-3000`, sibling repo at `/home/ivanhrabcak/Repositories/spevnikovac-3000`) fetch Ultimate Guitar songs through its working Cloudflare-bypass webview, open pre-seeded with the hardcoded `songs.txt` list, and export fetched songs as ChordPro (in addition to the existing docx export).

**Architecture:** Push this session's backend commits to GitHub so the frontend's git dependency sees them. Expose `write_chordpro` as a `#[tauri::command]`. Migrate the frontend from Tauri v1 to v2 (the backend already made this jump) using the official migration tool plus documented manual fixes. Add the missing v2 capability that grants the Ultimate Guitar fetch window IPC access — this is the actual fix for the "browser in the background doesn't work" problem, since v1's dynamic runtime grant has no v2 equivalent. Add a hardcoded song list + "fetch all" flow to the add-songs screen, and a ChordPro export option to the export screen.

**Tech Stack:** Rust (backend crate `spevnik`, Tauri v2), TypeScript/React (frontend, Vite), Tauri v2's capability/ACL system.

## Global Constraints

- Backend: only `write_chordpro` gets a new `#[tauri::command]` annotation — no other backend design changes.
- Frontend's `spevnik` dependency stays a **git** dependency (unchanged in `src-tauri/Cargo.toml`) — it must be refreshed via `cargo update -p spevnik` after the backend push, not switched to a path dependency.
- "Fetch all" runs **sequentially**, not in parallel (the UG fetch opens a real webview window per call).
- No persistence of `SongsContext` between app runs — pre-seeding means pre-seeding the *candidate URL list* on the add-songs screen, not restoring previously-fetched song data.
- A batch fetch failure on one URL must not abort the rest of the batch.

---

### Task 1: Backend — expose `write_chordpro`, push to GitHub

**Files:**
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000-backend/src/export.rs`

**Interfaces:**
- Produces: `#[tauri::command] pub fn write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String>` (same signature as today, now a command), used by Task 2's frontend command registration and Task 5's export UI.

- [ ] **Step 1: Add the command annotation**

In `/home/ivanhrabcak/Repositories/spevnikovac-3000-backend/src/export.rs`, add `#[tauri::command]` directly above the existing `write_chordpro` function (no other changes to its body):

```rust
#[tauri::command]
pub fn write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String> {
```

- [ ] **Step 2: Verify it compiles**

Run (from `/home/ivanhrabcak/Repositories/spevnikovac-3000-backend`): `cargo build`
Expected: builds with no new errors (pre-existing unused-import warnings expected).

- [ ] **Step 3: Commit**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000-backend
git add src/export.rs
git commit -m "Expose write_chordpro as a tauri command"
```

- [ ] **Step 4: Push to GitHub**

```bash
git push origin master
```

Expected: pushes all commits from `dfc27b0` through the new `write_chordpro` command commit. Confirm with `git log origin/master -1 --oneline` matching local `git log -1 --oneline`.

---

### Task 2: Frontend — migrate to Tauri v2

**Files:**
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/Cargo.toml`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/Cargo.lock`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/tauri.conf.json`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/src/main.rs`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/package.json`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/yarn.lock`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/add-songs.tsx`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/export.tsx`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/components/ChordsEditor.tsx`
- New: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/capabilities/default.json` (or whatever the migrate tool names it)

**Interfaces:**
- Consumes: Task 1's pushed `write_chordpro` command (needs `cargo update -p spevnik` in `src-tauri` to pick up).
- Produces: a frontend that builds and runs on Tauri v2, with `write_chordpro` registered in the command handler alongside the existing four commands, ready for Task 3 (capability) and Task 5 (export UI) to build on.

**Context:** This is a real, empirical migration — the exact diff the migrate tool produces isn't fully predictable, so this task is "run the tool, then fix what it doesn't cover, verified by the build." That's a legitimate way to execute this: don't guess at exact file contents; run the commands, read the actual errors, fix them, repeat until clean.

- [ ] **Step 1: Run the official migration tool**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
yarn add -D @tauri-apps/cli@latest
yarn tauri migrate
```

This auto-converts `src-tauri/tauri.conf.json` (v1 `allowlist`/`package`/`tauri.*` → v2 `productName`/`version`/`app.windows`/`app.security`/`bundle`), bumps `src-tauri/Cargo.toml`'s `tauri`/`tauri-build` to `2`, bumps `package.json`'s `@tauri-apps/api` to `^2`, and generates a `src-tauri/capabilities/` directory with a default capability. Read what it actually produced (`git diff` in the frontend repo) before continuing — don't assume it matches any example verbatim.

- [ ] **Step 2: Add the dialog plugin**

v1's built-in `dialog` allowlist feature became a separate plugin in v2. Add it on both sides:

```bash
yarn add @tauri-apps/plugin-dialog
cd src-tauri && cargo add tauri-plugin-dialog && cd ..
```

In `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/src/main.rs`, register the plugin and the new command. The file currently looks like:

```rust
use spevnik::export::{
    __cmd__fetch, __cmd__get_editing_hints, __cmd__report_ug_page, __cmd__transpose,
    __cmd__write_docx, fetch, get_editing_hints, report_ug_page, transpose, write_docx,
};

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            fetch,
            get_editing_hints,
            write_docx,
            transpose,
            report_ug_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Update the import to include `write_chordpro` (Tauri v2's command macro no longer needs the `__cmd__*` wrapper imports — check the actual compiler errors after bumping the dependency and remove them if they're now unresolved; v2 command registration only needs the plain function names in `generate_handler!`), add `.plugin(tauri_plugin_dialog::init())`, and add `write_chordpro` to the handler list:

```rust
use spevnik::export::{fetch, get_editing_hints, report_ug_page, transpose, write_chordpro, write_docx};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            fetch,
            get_editing_hints,
            write_docx,
            write_chordpro,
            transpose,
            report_ug_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

If `cargo build` (from `src-tauri`) reports the `__cmd__*` imports are still needed or that the plain names don't work, that means v2's macro-generated names differ from this guess — read the actual compiler error and use whatever it reports as the correct set of items to import; don't guess a second time, use the compiler's own suggestion.

- [ ] **Step 3: Fix the four `invoke` import sites**

Tauri v2 moved the core module from `@tauri-apps/api/tauri` (or the flat `@tauri-apps/api` entry point) to `@tauri-apps/api/core`. In each of these four files, change:

```typescript
import { invoke } from "@tauri-apps/api";
```
to:
```typescript
import { invoke } from "@tauri-apps/api/core";
```

Files: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/add-songs.tsx`, `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/export.tsx`, `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/components/ChordsEditor.tsx` (two `invoke` calls in this file, one import line).

- [ ] **Step 4: Fix the dialog import in `export.tsx`**

In `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/export.tsx`, change:

```typescript
import { save } from "@tauri-apps/api/dialog";
```
to:
```typescript
import { save } from "@tauri-apps/plugin-dialog";
```

Leave the `desktopDir`/`join` import from `@tauri-apps/api/path` unchanged — that module stayed in core in v2, it was not moved to a plugin.

- [ ] **Step 5: Verify the capability covers the existing four commands**

Open the capability file the migrate tool generated under `src-tauri/capabilities/`. Confirm it (or add to it) grants the main window permission to call `fetch`, `get_editing_hints`, `write_docx`, `write_chordpro`, and `transpose` — Tauri v2's command macro auto-generates a permission per command (named after the crate, e.g. `spevnik:allow-fetch`); check `src-tauri/target/debug/build/*/out/` or the build output for the actual generated permission identifiers if unsure, and add `dialog:default` for the dialog plugin. Do not add the `report_ug_page` / remote-domain permission here — that's Task 3.

- [ ] **Step 6: Refresh the `spevnik` dependency and build**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri
cargo update -p spevnik
cargo build
```

Expected: builds with no errors (confirms Task 1's `write_chordpro` command is visible and the v2 migration compiles). Fix any remaining compiler errors iteratively — they will name the exact problem; resolve each against the v2 docs pattern shown above rather than guessing.

- [ ] **Step 7: Verify the frontend type-checks**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
yarn build
```

Expected: `tsc` + `vite build` succeed with no new type errors. Fix any remaining import/type issues the same way — read the actual error, apply the documented v2 equivalent.

- [ ] **Step 8: Commit**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
git add -A
git commit -m "Migrate to Tauri v2"
```

---

### Task 3: Frontend — grant the Ultimate Guitar fetch window IPC access

**Files:**
- New: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri/capabilities/ug-fetch.json`

**Interfaces:**
- Consumes: the `ug-fetch-{N}` window label pattern from this backend's `src/export.rs` (`next_fetch_label()` produces labels like `ug-fetch-0`, `ug-fetch-1`, ...), and the `report_ug_page` command.

**Context:** This is the actual fix for "the browser in the background doesn't work." Tauri v1 granted this dynamically at runtime per-window (`app.ipc_scope().configure_remote_access(...)`, removed in the v2 migration commit `0ce74fc` in the backend). Tauri v2 requires it declared statically. The exact permission identifier for `report_ug_page` needs to be confirmed empirically (see Task 2 Step 5) — it does not need re-deriving here if Task 2 already found it.

- [ ] **Step 1: Write the capability**

```json
{
  "identifier": "ug-fetch",
  "description": "Allow the Ultimate Guitar Cloudflare-bypass window to report the fetched page back to report_ug_page",
  "windows": ["ug-fetch-*"],
  "remote": {
    "urls": ["https://*.ultimate-guitar.com/*", "https://ultimate-guitar.com/*"]
  },
  "permissions": ["spevnik:allow-report-ug-page"]
}
```

If `spevnik:allow-report-ug-page` isn't the real generated identifier (confirm against Task 2 Step 5's findings, or `grep -r "report_ug_page" src-tauri/target/debug/build/*/out/` after a build), use the real one instead.

- [ ] **Step 2: Verify it's picked up**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000/src-tauri
cargo build
```

Expected: no ACL/capability validation errors (Tauri validates capability files at build time — an unknown permission identifier fails the build with a clear message naming the problem).

- [ ] **Step 3: Manual end-to-end verification**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
yarn tauri dev
```

In the running app, paste a real Ultimate Guitar URL into the add-songs form (e.g. one from `songs.txt`, such as `https://tabs.ultimate-guitar.com/tab/eric-clapton/tears-in-heaven-chords-627220`) and submit. Expected: the fetch succeeds and the song appears in the list — this proves the bypass window's injected script successfully called `report_ug_page` through the new capability. If it fails, read the actual error (browser console via right-click → Inspect in the bypass window, or the Rust-side error surfaced in the UI) rather than re-guessing the capability shape blind.

- [ ] **Step 4: Commit**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
git add src-tauri/capabilities/ug-fetch.json
git commit -m "Grant the Ultimate Guitar fetch window IPC access to report_ug_page"
```

---

### Task 4: Frontend — hardcoded song list + "fetch all"

**Files:**
- New: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/songs-list.ts`
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/add-songs.tsx`

**Interfaces:**
- Produces: `export const SONGS_LIST: string[]` from `songs-list.ts`, consumed by `add-songs.tsx`.

- [ ] **Step 1: Create the hardcoded list**

```typescript
export const SONGS_LIST: string[] = [
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
  "https://www.supermusic.cz/piesen.php?idpiesne=997232",
];
```

This is a literal copy of this backend repo's `songs.txt`, kept in sync by hand (matching the project's existing pattern of duplicating the hardcoded list rather than sharing a file between the two repos).

- [ ] **Step 2: Add "fetch all" to `add-songs.tsx`**

The current file (`/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/add-songs.tsx`) has a single-URL form calling `addSong()`. Add, alongside it: a queued-URL list seeded from `SONGS_LIST`, a per-URL status tracker, and a sequential "fetch all" action. Add these imports and this state/logic to the component (the existing `addSong` function and its surrounding JSX form stay unchanged):

```typescript
import { SONGS_LIST } from "../songs-list";

// inside AddSongsRoute, alongside the existing useState calls:
const [queuedUrls] = useState<string[]>(SONGS_LIST);
const [batchStatus, setBatchStatus] = useState<Record<string, "pending" | "success" | "error">>({});
const [isBatchRunning, setBatchRunning] = useState(false);

const fetchAll = async () => {
  setBatchRunning(true);
  const initialStatus: Record<string, "pending" | "success" | "error"> = {};
  queuedUrls.forEach((u) => (initialStatus[u] = "pending"));
  setBatchStatus(initialStatus);

  const collected: Record<string, Song> = {};

  for (const queuedUrl of queuedUrls) {
    try {
      const result = (await invoke("fetch", { url: queuedUrl })) as LyricsWithChords;
      collected[`${result.song_name} - ${result.artist}`] = {
        nodes: result.text,
        transposedBy: 0,
      };
      setBatchStatus((prev) => ({ ...prev, [queuedUrl]: "success" }));
      setSongs((prevSongs) => ({ ...prevSongs, ...collected }));
    } catch (e) {
      setBatchStatus((prev) => ({ ...prev, [queuedUrl]: "error" }));
    }
  }

  setBatchRunning(false);
};
```

`Song` is already imported from `../components/context/songs-context` in this file (used by the existing `newItem: Record<string, Song>` in `addSong`) — reuse that import, don't add a duplicate.

Add a "Fetch all" button and a simple per-URL status list to the JSX, e.g. directly below the existing single-URL form:

```tsx
<div className="flex flex-col items-center w-[50%] gap-2 mt-4">
  <button className="btn" disabled={isBatchRunning} onClick={fetchAll}>
    {isBatchRunning ? "Načítavam..." : `Načítať všetky (${queuedUrls.length})`}
  </button>
  {Object.keys(batchStatus).length > 0 && (
    <div className="w-full max-h-48 overflow-y-auto text-sm">
      {queuedUrls.map((u) => (
        <div key={u} className="flex justify-between gap-2">
          <span className="truncate">{u}</span>
          <span>
            {batchStatus[u] === "pending" && "..."}
            {batchStatus[u] === "success" && "✓"}
            {batchStatus[u] === "error" && "✗"}
          </span>
        </div>
      ))}
    </div>
  )}
</div>
```

- [ ] **Step 3: Verify it type-checks and runs**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
yarn build
```

Expected: no new type errors.

- [ ] **Step 4: Commit**

```bash
git add src/songs-list.ts src/routes/add-songs.tsx
git commit -m "Pre-seed add-songs screen with the hardcoded song list and a fetch-all action"
```

---

### Task 5: Frontend — ChordPro export option

**Files:**
- Modify: `/home/ivanhrabcak/Repositories/spevnikovac-3000/src/routes/export.tsx`

**Interfaces:**
- Consumes: `write_chordpro(songs: Vec<LyricsWithChords>, path: String) -> Result<(), String>` (Task 1), which now writes one `.cho` file per song into a directory — so this needs a **folder** picker, not a file picker.

- [ ] **Step 1: Add a ChordPro export action**

The current `export.tsx` has a `savePath` state (a file path for docx) and a `startExport` function calling `invoke("write_docx", ...)`. Add a second, independent save-path and export action for ChordPro, using `@tauri-apps/plugin-dialog`'s `open({ directory: true })` instead of `save()` (since the output is a folder of files, not one file):

```typescript
import { open, save } from "@tauri-apps/plugin-dialog";

// alongside the existing savePath/isLoading/isDone state:
const [chordproDir, setChordproDir] = useState<string | null>(null);
const [isChordproLoading, setChordproLoading] = useState(false);
const [isChordproDone, setChordproDone] = useState(false);

const startChordproExport = async () => {
  if (chordproDir == null) return;

  setChordproLoading(true);
  const mappedChords = Object.keys(songs).map((k) => {
    const [artist, song_name] = k.split(" - ");
    const text = songs[k];

    return { artist, song_name, text: text.nodes };
  });

  await invoke("write_chordpro", {
    songs: mappedChords,
    path: chordproDir,
  });

  setChordproLoading(false);
  setChordproDone(true);
};
```

Add a button to pick the folder and trigger the export, e.g. below the existing docx export button:

```tsx
<div className="flex flex-col items-center w-full gap-3 mt-5">
  <button
    type="button"
    className="btn"
    onClick={async () => {
      const dir = await open({ directory: true });
      if (typeof dir === "string") setChordproDir(dir);
    }}
  >
    {chordproDir ?? "Vybrať priečinok pre ChordPro"}
  </button>
  <button
    type="button"
    className="btn btn-secondary"
    disabled={chordproDir == null || isChordproDone}
    onClick={startChordproExport}
  >
    {isChordproLoading && <span className="loading relative loading-spinner loading-md" />}
    {!isChordproLoading && "Exportovať ako ChordPro"}
  </button>
</div>
```

Reuse the existing `mappedChords`-building logic pattern already present in `startExport` — both actions build the same shape from `songs`, duplicated per-function to match the existing file's style (it doesn't currently factor this out either).

- [ ] **Step 2: Verify it type-checks**

```bash
cd /home/ivanhrabcak/Repositories/spevnikovac-3000
yarn build
```

Expected: no new type errors.

- [ ] **Step 3: Manual verification**

```bash
yarn tauri dev
```

Fetch at least one song (via the single-add form or "fetch all"), navigate to `/export`, pick a folder for ChordPro export, click "Exportovať ako ChordPro". Expected: the chosen folder contains one `.cho` file per fetched song, named `"{artist} - {song_name}.cho"`.

- [ ] **Step 4: Commit**

```bash
git add src/routes/export.tsx
git commit -m "Add ChordPro export option to the export screen"
```
