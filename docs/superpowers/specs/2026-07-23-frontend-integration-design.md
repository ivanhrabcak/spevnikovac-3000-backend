# Frontend integration: Tauri v2 + hardcoded song list + ChordPro export — design

## Goal

Make the real desktop app (`spevnikovac-3000`, a sibling repo at
`/home/ivanhrabcak/Repositories/spevnikovac-3000`) able to:

1. Actually fetch Ultimate Guitar songs — the Cloudflare-bypass webview
   trick (`fetch_ultimate_guitar_html` in this backend's `src/export.rs`)
   only works inside a real running Tauri app; the backend's headless
   `main.rs` can never run it. `main.rs`'s plain-`reqwest` fetch path was
   never able to get past Ultimate Guitar's Cloudflare challenge (confirmed
   empirically: a real run panicked on the first UG URL).
2. Open pre-seeded with the same hardcoded song list used in this backend's
   `main.rs` (from `songs.txt`), so fetching all of them is a couple of
   clicks instead of pasting 46 URLs by hand.
3. Export the fetched songs as ChordPro, not just docx.

## Background

The frontend is currently pinned to Tauri v1 throughout: `src-tauri/Cargo.toml`
(`tauri = "1"`), `package.json` (`@tauri-apps/api ^1`, `@tauri-apps/cli ^1`),
`src-tauri/tauri.conf.json` (v1's `allowlist`/`security.csp` schema). Its
`spevnik` dependency is a **git** dependency pointing at
`https://github.com/ivanhrabcak/spevnikovac-3000-backend` — it does not see
this session's local backend commits until they're pushed.

This backend was migrated to Tauri v2 earlier in this session (commit
`0ce74fc`). That migration left one known gap, called out in that commit's
message: v1's dynamic `app.ipc_scope().configure_remote_access(...)` (which
let `fetch_ultimate_guitar_html` grant a freshly-opened window IPC access to
call `report_ug_page` back) has no v2 equivalent — v2 requires this to be
declared statically as a **capability**, which belongs in the app (frontend),
not the library (backend). That capability has never been added anywhere,
which is the concrete reason "the browser in the background" doesn't work
today even conceptually — there was no app for it to work inside of before
now, and even once there is, nothing grants it IPC access yet.

Four `#[tauri::command]`s already exist in the backend and are already wired
into the frontend's `src-tauri/src/main.rs` handler list: `fetch`,
`get_editing_hints`, `write_docx`, `transpose`, plus the internal
`report_ug_page`. `write_chordpro` (added in this session, currently a plain
`pub fn`, not a command) is not yet exposed or wired up anywhere.

The frontend's flow: `AddSongsRoute` (`/`) lets the user paste one URL at a
time, calling `invoke("fetch", { url })` and storing the result in
`SongsContext` (in-memory React state only, `Record<string, Song>` keyed by
`"{song_name} - {artist}"`). `EditSongsRoute` (`/edit`) lets the user edit
chords per song. `ExportRoute` (`/export`) calls
`invoke("write_docx", { songs, path })` to a user-chosen path via
`@tauri-apps/api/dialog`'s `save()`.

## Design

### 1. Push backend commits to GitHub

Push this session's 7 commits (`dfc27b0`..`a6f7e07`, the ChordPro export
work and the Tauri v2 migration) to `spevnikovac-3000-backend`'s `master` on
GitHub, so the frontend's existing git dependency can pick them up via
`cargo update -p spevnik` (no `Cargo.toml` change needed on the frontend
side — it already points at this repo, just needs a lockfile refresh).

### 2. Backend: expose `write_chordpro` as a command

Add `#[tauri::command]` to `write_chordpro` in `src/export.rs` (it already
has the right signature: `Vec<LyricsWithChords>, String -> Result<(), String>`,
matching `write_docx`'s shape). This reverses the earlier "plain fn only"
constraint from this session's first design — that constraint was scoped to
"no frontend support was planned yet"; the frontend is now in scope by the
user's explicit request. `render_chordpro` and the free function's logic are
unchanged.

### 3. Frontend: migrate to Tauri v2

Run the official migration tool (`npm run tauri migrate` — the `@tauri-apps/cli`
package needs bumping to `^2` first) to auto-convert
`src-tauri/tauri.conf.json` (v1 `allowlist`/`package`/`tauri.*` shape → v2's
top-level `productName`/`version`, `app.windows`, `app.security`, `bundle`),
`src-tauri/Cargo.toml` (`tauri`/`tauri-build` → `2`), and `package.json`
(`@tauri-apps/api` → `^2`). Then hand-fix what the tool doesn't cover:

- `src-tauri/src/main.rs`: register the `write_chordpro` command; register
  the dialog plugin (`tauri_plugin_dialog::init()`) since v1's built-in
  `dialog` allowlist feature became a separate plugin in v2.
- `package.json`: add `@tauri-apps/plugin-dialog` (the `save()` dialog API
  moved out of `@tauri-apps/api` into this plugin in v2).
- Four `invoke(...)` call sites (`add-songs.tsx`, `ChordsEditor.tsx` ×2,
  `export.tsx`) change their import from `@tauri-apps/api` to
  `@tauri-apps/api/core`.
- `export.tsx`'s `save` import changes from `@tauri-apps/api/dialog` to
  `@tauri-apps/plugin-dialog`; `desktopDir`/`join` stay at
  `@tauri-apps/api/path` (that module stayed in core in v2, unlike dialog).
- `src-tauri/capabilities/default.json` (new, or whatever the migrate tool
  generates): the main window's capability needs `core:default` plus
  `dialog:default` and the four command permissions
  (`fetch`, `get_editing_hints`, `write_docx`, `write_chordpro`, `transpose`)
  — the migrate tool typically seeds a reasonable default capability; this
  step is "verify/adjust it," not "write from scratch."

### 4. Frontend: the missing remote-IPC capability

Add a capability file (e.g. `src-tauri/capabilities/ug-fetch.json`) granting
windows labeled `ug-fetch-*` permission to call `report_ug_page`, extended to
content loaded from Ultimate Guitar's domain via `remote.urls`:

```json
{
  "identifier": "ug-fetch",
  "description": "Allow the Ultimate Guitar Cloudflare-bypass window to report the fetched page back",
  "windows": ["ug-fetch-*"],
  "remote": {
    "urls": ["https://*.ultimate-guitar.com/*"]
  },
  "permissions": ["spevnik:allow-report-ug-page"]
}
```

The exact permission identifier (`spevnik:allow-report-ug-page` above is a
guess at the naming convention) needs to be confirmed against what the
`#[tauri::command]` macro / `tauri-build` actually generates for a command
from a dependency crate — this is exactly the kind of detail the
implementation step needs to verify empirically (build, read the generated
permission files under `src-tauri/gen/schemas/`, or check the command's
auto-generated ACL) rather than the design guessing correctly upfront. If
per-command auto-generated permissions turn out not to work cleanly for a
command defined in a dependency crate rather than the app crate itself, the
fallback is a capability granting broader `core:event:default` /
custom IPC scope — resolved during implementation, flagged here as a known
unknown.

### 5. Frontend: hardcoded song list + "fetch all"

A new file, `src/songs-list.ts`, exporting the same 46 URLs as this
backend's `songs.txt` (a literal TS array — no shared source file between
the two repos; duplication accepted, matching the project's existing
"hardcoded list, in two places" pattern between `songs.txt` and `main.rs`).

`add-songs.tsx` changes:
- On mount, if `songs` is empty and no fetch is in progress, pre-seed a new
  local `queuedUrls` state from `songs-list.ts` (does not touch
  `SongsContext` yet — nothing is fetched until the user acts).
- The existing single-URL add form stays as-is (manual add still works).
- A new "Fetch all" button iterates `queuedUrls` sequentially (not
  parallel — the webview-based UG fetch opens a real window per call;
  running 46 at once would be chaotic), calling `invoke("fetch", { url })`
  for each. Per-URL outcome (success/error) is tracked in local state and
  rendered as a simple list (URL + status), so a failure is visible without
  interrupting the rest of the batch. Successes are merged into
  `SongsContext` as they complete, the same way the existing single-add
  flow does.

### 6. Frontend: ChordPro export option

`export.tsx` gets a second export action next to the existing "Uložiť"
(docx) button: "Exportovať ako ChordPro," which calls
`invoke("write_chordpro", { songs: mappedChords, path })`. Since
`write_chordpro` now writes one file per song into a directory (per this
session's Task 4 revision), the save dialog needs to pick a **folder**, not
a file — `@tauri-apps/plugin-dialog`'s `open({ directory: true })` instead
of `save()`. Reuses the same `mappedChords` construction already in
`startExport`.

## Testing

- Backend: `cargo build` after adding `#[tauri::command]` to `write_chordpro`
  (trivial, no new logic).
- Frontend: `tsc` / `yarn build` for type-checking after the v2 migration and
  import-path changes.
- Manual, via `yarn tauri dev`: confirm the app opens, the add-songs screen
  shows the pre-seeded list, "Fetch all" successfully fetches at least one
  real Ultimate Guitar song through the bypass window (this is the actual
  proof the capability fix works — it could not be verified any other way
  in this session, since it requires a real running Tauri app), and ChordPro
  export produces a folder of `.cho` files.

## Out of scope

- No changes to `main.rs`'s own fetch loop (its Cloudflare-blocked UG path,
  or its `.unwrap()`-panics-on-first-failure behavior) — the frontend is now
  the intended path for actually fetching Ultimate Guitar songs; `main.rs`
  remains a supermusic.cz-only-realistic tool.
- No persistence of `SongsContext` between app runs (still in-memory only,
  matching existing behavior) — "pre-seeded on open" means pre-seeding the
  *candidate URL list*, not restoring previously-fetched song data.
- No parallelization of the "fetch all" batch.
- No UI treatment beyond a minimal per-URL status list for the batch fetch —
  no retry button, no cancel-mid-batch (can revisit if it proves painful in
  practice).
