# 🦊 fox-cull

**A fast, lightweight photo & video culler for Windows and macOS.**
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
- **Rate and flag at speed** — star ratings (1–5), color labels, and a
  keep/reject flag, all from the keyboard.
- **Big, clean preview** with a Lightroom-style film-strip along the bottom.
- **Photos always upright** — no more sideways portrait shots from your phone.
- **Handles RAW** — Nikon `.NEF` and other RAW files preview instantly.
- **Videos too** — phone clips play right in the app (best-effort).
- **Filter, then bulk-reject** — e.g. show everything below 3 stars, select
  all, reject, then sweep the rejects to the Recycle Bin in one go.

## Download & install

Grab the latest build from the **[Releases page](../../releases)**:

- **Windows** — `fox-cull_*_x64-setup.exe`. On first launch Windows SmartScreen
  may warn (the app isn't code-signed yet): click **More info → Run anyway**.
- **macOS (Apple Silicon)** — `fox-cull_*_aarch64.dmg`. It isn't notarized yet,
  so on first launch **right-click the app → Open**, then confirm. (If macOS
  says it's "damaged", open Terminal and run
  `xattr -dr com.apple.quarantine /Applications/fox-cull.app`.)
- **Linux** — `.AppImage` (portable) or `.deb`.

## How to cull

1. Click **Folder…** (top-left) and choose the folder or drive with your photos.
2. Click a folder in the tree — the grid fills with everything inside it.
3. Use the keyboard to fly through:

| Key | Action |
|---|---|
| `←` / `→` | Previous / next photo |
| `Enter` | Toggle big "loupe" view |
| `1`–`5` | Star rating |
| `` ` `` | Clear rating |
| `6` `7` `8` `9` `0` | Color label (blue / purple / red / green / yellow) |
| `X` | Reject |
| `P` | Pick (keep) |
| `U` | Clear everything on this photo |

4. Use the **filter bar** to narrow down (e.g. only rejected, or 3+ stars),
   **Select all**, and **Reject** them in bulk.
5. When you're ready, hit **Delete rejected…** — the rejects move to your
   Recycle Bin / Trash (recoverable), freeing up space.

Tip: the **⊞ Subfolders** button (on by default) controls whether a folder
shows photos from its subfolders too.

## Good to know

- **Ratings live in fox-cull only.** They're stored in the app's own database,
  not written into your photo files, so they won't show up in other apps (and
  won't touch your originals).
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
