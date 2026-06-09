# 🦊 fox-cull

**A fast, lightweight photo & video culler for Windows, macOS & Linux.**
Sort through thousands of photos quickly — keep the good ones, reject the rest —
without a subscription, without the bloat.

fox-cull is built for the part of photography nobody enjoys: the first pass
through a huge pile of shots from your phone and camera, deciding what's worth
keeping. It borrows the workflow you already know from Lightroom — a folder
list, a big preview, a film-strip along the bottom, star ratings and color
labels — and does just that one job, fast.

> **Your photos are never modified.** fox-cull reads your files where they sit
> (on your drive or an external SSD) and remembers your ratings in its own
> database. Nothing is imported, copied, or written next to your originals
> until *you* choose to delete the rejects — and even then they go to the
> Recycle Bin / Trash, so they're recoverable.

---

## What it does

- **Browse any folder in place** — point it at a drive or folder and it shows
  the whole tree. Pick a folder and you see every photo inside it *and* its
  subfolders, all at once.
- **Three ways to look** — **Grid** (adjustable thumbnail size), **Details**
  (name / type / size / date list, like a file explorer), and **Focus** (one
  big photo) — switch any time, top-left.
- **Rate, flag, and tag at speed** — star ratings (1–5), color labels, a
  keep/reject flag, and your own free-form **tags** (e.g. "Diwali"), all from
  the keyboard. Filter by any of them.
- **Sort & group by real capture date** — order by the date a shot was actually
  *taken* (from EXIF / video metadata), not the filename, and optionally split
  the grid into **month** or **week** sections.
- **Big, clean preview** with a Lightroom-style film-strip (bottom, side, or
  off — and drag to resize it). New photos fade in sharp from a soft preview —
  no jarring thumbnail-to-full pop.
- **Right-click anything** for a contextual menu — open in your file manager or
  system player, jump to the next/previous shot, pick / reject, copy the path.
- **Dim / lights-out focus mode** (`L`) darkens everything but the photo.
- **Photos always upright** — no more sideways portrait shots from your phone.
- **Fast on huge phone JPEGs** — thumbnails decode at reduced scale and the
  whole folder pre-loads in the background, so scrolling stays smooth.
- **Handles RAW** — Nikon `.NEF` and other RAW files preview instantly.
- **Real video support** — every clip gets a true poster frame, an in-app player
  with a **scrub timeline** (hover to preview frames), and **lossless trim &
  export** (set in/out points, cut without re-encoding). HEVC clips (e.g. DJI
  Osmo Pocket 3) still get posters and scrubbing, and open in your system player
  in one click if the webview can't decode them. `Space` plays/pauses,
  `Shift`+`←`/`→` seeks.
- **Filter, then bulk-reject** — e.g. show everything below 3 stars, select
  all, reject, then sweep the rejects to the Recycle Bin in one go.
- **Per-drive & portable** — each drive keeps its own catalog, preview cache and
  recycle bin together in a `_FoxCull` folder at its root, so your ratings and
  deletions travel with the drive between machines. The app itself can run
  portable from a USB stick / SSD too (see below). See
  [STORAGE.md](STORAGE.md) for exactly what's stored where.

## Download & install

Grab the latest build from the **[Releases page](../../releases)**:

- **Windows** — `fox-cull_*_x64-setup.exe`. On first launch Windows SmartScreen
  may warn (the app isn't code-signed yet): click **More info → Run anyway**.
- **macOS (Apple Silicon)** — `fox-cull_*_aarch64.dmg`. It isn't notarized yet,
  so on first launch **right-click the app → Open**, then confirm. (If macOS
  says it's "damaged", open Terminal and run
  `xattr -dr com.apple.quarantine /Applications/fox-cull.app`.)
- **Linux** — `.AppImage` (portable) or `.deb`.
- **Windows portable** — `fox-cull_*_x64_portable.zip`. Unzip anywhere (e.g.
  onto your SSD). It contains `fox-cull.exe` and a `fox-cull-data` folder; keep
  them together and the app stores its catalog, cache and settings in that
  folder instead of in Windows AppData — so the whole app travels with you.
  (Needs the Microsoft WebView2 runtime, which ships with Windows 10/11.)

## How to cull

1. Click **Folder…** (top-left) and choose the folder or drive with your photos.
2. Click a folder in the tree — the grid fills with everything inside it.
3. Use the keyboard to fly through:

| Key | Action |
|---|---|
| `←` / `→` | Previous / next photo |
| `Enter` | Toggle big **Focus** view |
| `G` / `D` | **Grid** / **Details** view |
| `L` | Dim → lights-out → normal (focus mode) |
| `Space` | Play / pause the current video |
| `Shift`+`←` / `Shift`+`→` | Scrub the video back / forward |
| `1`–`5` | Star rating |
| `` ` `` | Clear rating |
| `6` `7` `8` `9` `0` | Color label (blue / purple / red / green / yellow) |
| `X` | Reject |
| `P` | Pick (keep) |
| `U` | Clear everything on this photo |

4. Use the **filter bar** to narrow down (e.g. only rejected, 3+ stars, or a
   tag), **Select all**, and **Reject** them in bulk. Add a tag to the selected
   photo(s) by typing in the **+ tag** box in the bottom info bar.
5. When you're ready, **hold the Delete button** to sweep the rejects. By default
   they go to the **in-app Trash** (a `_FoxCull/recycle` folder on that drive) —
   open **♻ Trash** any time to preview them, **restore** one to its original
   spot, or **delete forever**. Prefer the OS bin? Switch "On delete" to
   **System Recycle Bin** in Settings (⚙).

Tip: **⊞ Subfolders** (in the **Filters** menu, on by default) controls whether
a folder shows photos from its subfolders too. Use the **Sort** and **Group**
controls next to the view switcher to order by capture date or split the grid
into month / week sections.

## Good to know

- **Ratings live in fox-cull only.** They're stored in the app's own catalog (a
  `_FoxCull` folder on each drive), not written into your photo files, so they
  won't show up in other apps and won't touch your originals. Because the catalog
  lives on the drive, it backs up with your photos and follows the drive to
  another computer. Full details: [STORAGE.md](STORAGE.md).
- **Read-only drives:** if a drive is mounted read-only (e.g. an NTFS SSD on a
  Mac without a write driver), culling and rating still work — only the final
  delete sweep is disabled, with a note explaining why.
- **Big RAW / very-high-resolution photos** take a moment to preview the first
  time; after that they're cached and instant.

## Build from source

fox-cull is built with [Tauri 2](https://tauri.app) + SvelteKit + Rust.

```bash
npm install
npm run tauri dev      # run locally
```

Tagged releases (`v*`) build Windows, macOS, and Linux installers automatically
via GitHub Actions.

## License

[MIT](LICENSE) © Kumar Adarsh
