# Where fox-cull stores things

fox-cull never modifies, imports, or copies your original photos and videos. It
reads them where they sit and keeps everything it generates — your ratings, the
preview cache, and deleted files — in **one self-contained folder per drive**, so
each drive's data travels with it and you always know exactly what the app wrote
and where.

> **Your originals are never touched** until *you* delete a reject — and even then
> the file is moved to a recoverable Trash, not erased.

## The per-drive library (`_FoxCull`)

Each drive/volume you browse gets its own library folder at its root:

```
<drive root>/                     e.g.  E:\   or   /Volumes/Photos SSD/
  _FoxCull/
    catalog.sqlite                ← ratings, labels, flags, tags, trims, capture dates
    thumbs/                       ← cached thumbnails, Focus previews & video posters
    recycle/                      ← files you deleted (the in-app Trash), mirroring
                                     their original folder structure
```

When you open a folder, fox-cull automatically uses the library of the drive that
folder lives on — there's no "open catalog" step. Browse the SSD and it uses the
SSD's `_FoxCull`; switch to an internal drive and it uses that drive's. Everything
for a drive (catalog **+** cache **+** recycle) stays together on that drive, so
you can unplug it, move it, or carry it to another machine and your ratings and
deletions come with it.

The library folder is hidden from the grid — fox-cull never shows `_FoxCull`
contents as photos to cull.

### Read-only drives (fallback)

If a drive root isn't writable — e.g. an NTFS SSD mounted read-only on a Mac
without a Paragon/Tuxera driver, or a locked system drive — fox-cull can't create
`_FoxCull` there. It falls back to a per-drive folder under the app's data
directory instead, so **rating and culling still work** even on a read-only mount:

```
<app data>/libraries/<drive-id>/{catalog.sqlite, thumbs, recycle}
```

On such a mount the **delete sweep is disabled** (the files themselves can't be
moved), with a note in the app explaining why. Do the deleting later on a machine
where the drive is writable.

App-data location:
- **Windows:** `%APPDATA%\com.foxcull.app\`
- **macOS:** `~/Library/Application Support/com.foxcull.app/`
- **Linux:** `~/.config/com.foxcull.app/` (or `$XDG_CONFIG_HOME`)

### Portable mode

If a folder named `fox-cull-data` sits next to the `fox-cull.exe` (the Windows
portable build ships this way), the app keeps its config and the *default*
library there instead of app-data — so the whole app plus its data travels on a
USB stick. Per-drive `_FoxCull` libraries still apply for any media drive you open.

## The three kinds of data

| What | Where | Safe to delete? |
|---|---|---|
| **Catalog** (`catalog.sqlite`) | `<drive>/_FoxCull/` | No — this *is* your ratings/flags/tags. Back it up. |
| **Preview cache** (`thumbs/`) | `<drive>/_FoxCull/` | Yes — it's regenerated on demand. Deleting it only costs a one-time re-decode. |
| **Trash** (`recycle/`) | `<drive>/_FoxCull/` | Yes, but that permanently loses those deleted files. Use **Empty Trash** in-app. |

- The **catalog** is keyed by each file's path *relative to the drive root*, so it
  stays valid across machines (Windows `E:\…` vs Mac `/Volumes/…`). Nothing is
  written into your photo files or beside them.
- The **preview cache** filenames are content hashes of (path, modified-time,
  size, dimensions, orientation) — replacing or editing a file produces a fresh
  entry and old ones are simply never referenced again.

## Deleting & the in-app Trash

The toolbar's hold-to-delete sweeps everything you've flagged **Reject**. Two modes
(Settings → *On delete*):

- **In-app Trash** (default) — moves rejects into `<drive>/_FoxCull/recycle/`,
  preserving their folder structure. Open **♻ Trash** to see them as thumbnails
  (most recently rejected first), **Restore** them to their original location, or
  **Delete forever**. Recoverable entirely within fox-cull, no Explorer/Finder
  digging.
- **System Recycle Bin** — sends rejects to the OS Recycle Bin / Trash instead.
  Recover them through your file manager. (These are *not* listed in the in-app
  Trash, since the OS bin isn't reliably enumerable per-file.)

Restoring a file brings the file back; its previous rating/flags are not restored
(they were cleared when it was deleted).

## Migrating / moving a library

- **From an earlier fox-cull:** the first time you open a drive that had a legacy
  `fox-cull.catalog` (and `thumbs`) at its root, fox-cull adopts them into the new
  `_FoxCull` folder automatically. If your ratings lived in the app-data default
  catalog, that's copied in to seed the drive's library. Migration is
  **data-loss-safe** — the new catalog is written and opened *before* the old file
  is removed, so a disconnect mid-operation can never lose ratings.
- **Moving a drive's data:** just move (or copy) the whole `_FoxCull` folder with
  the drive. There is no in-app "move catalog" step anymore — the library is
  always wherever the drive is.

## Quick answers

- **"Where are my ratings?"** `<drive>/_FoxCull/catalog.sqlite`.
- **"What's taking up space?"** `<drive>/_FoxCull/thumbs/` (cache, safe to delete)
  and `<drive>/_FoxCull/recycle/` (your deleted files).
- **"I deleted something by mistake."** Open **♻ Trash** → select → **Restore**.
- **"Can I start fresh on a drive?"** Delete that drive's `_FoxCull` folder. You
  lose its ratings and Trash; your originals are untouched.
