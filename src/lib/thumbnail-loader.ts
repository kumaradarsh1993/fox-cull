// Viewport-prioritized, cancellable thumbnail loader.
//
// The grid's "not responding" freeze came from two things this module now fixes:
//
//  1. The old queue was FIFO with no cancellation. A fast scroll enqueued every
//     thumbnail you flew past; the cells now ON SCREEN waited behind that whole
//     backlog, so the grid looked frozen. We now serve the MOST RECENTLY asked
//     (i.e. the current viewport) FIRST (LIFO), and a cell that scrolls away
//     before its decode starts CANCELS its request — so work tracks the viewport.
//
//  2. Nothing kept decoded bitmaps warm, so every scroll-back re-fired an
//     asset-protocol IPC fetch + re-decode. A burst of `http://asset.localhost`
//     requests is exactly what janks the WebView2 UI thread. We now RETAIN a
//     large LRU of decoded <img> elements (the RAM the user explicitly offered),
//     so revisiting recent cells is an in-memory cache hit — zero IPC, no decode.
//
//  - caps concurrent decodes (never floods Rust with hundreds of 12MP decodes)
//  - memoizes resolved URLs (scroll-back is instant, zero IPC)
//  - DE-DUPLICATES in-flight requests so the grid + filmstrip share ONE decode
//  - generation token abandons queued work for the old folder on a switch

import { api } from "./api";

// 8 parallel decodes keeps all cores busy without thrashing the USB SSD's queue.
const MAX_INFLIGHT = 8;
// How many decoded thumbnails to keep warm in RAM. ~700 × ~0.3 MB ≈ 200 MB — well
// within the budget, and enough to cover scrolling a few screens back instantly.
const RETAIN = 700;

const memo = new Map<string, string>(); // key -> asset url (persists across folders)
const pending = new Map<string, Promise<string | null>>(); // key -> in-flight promise
type QItem = { key: string; run: () => void };
let queue: QItem[] = []; // served LIFO (newest request = current viewport first)
let inflight = 0;
let generation = 0;

// Decoded-image retention: holding a reference to an <img> keeps the webview's
// resource for that URL in memory, so a sibling <img> with the same src paints
// from cache without another asset-protocol round-trip. LRU-evicted by URL.
const decoded = new Map<string, HTMLImageElement>();
function retain(url: string) {
  const have = decoded.get(url);
  if (have) {
    decoded.delete(url);
    decoded.set(url, have); // mark most-recently-used
    return;
  }
  const img = new Image();
  img.decoding = "async";
  img.src = url;
  img.decode?.().catch(() => {});
  decoded.set(url, img);
  while (decoded.size > RETAIN) {
    const oldest = decoded.keys().next().value as string;
    decoded.delete(oldest);
  }
}

function pump() {
  while (inflight < MAX_INFLIGHT && queue.length) {
    queue.pop()!.run(); // LIFO: the most recently requested cell wins the slot
  }
}

/** Abandon queued (not-yet-started) work — call when the folder changes. */
export function resetThumbs() {
  generation++;
  queue = [];
  pending.clear();
  // Drop the warm decoded windows too — their images belong to the folder we're
  // leaving; releasing the references lets the webview reclaim that RAM.
  decoded.clear();
  loupeDecoded.clear();
  loupeInflight.clear();
}

/** Drop a single not-yet-started request (a grid/strip cell scrolled out of
 *  view before its decode began). In-flight requests are cheap to let finish. */
function cancel(key: string) {
  if (pending.has(key)) {
    const i = queue.findIndex((q) => q.key === key);
    if (i >= 0) {
      queue.splice(i, 1);
      pending.delete(key);
    }
  }
}

// ── loupe (Focus-view) preview prefetch ────────────────────────────────────
//
// The blur in Focus view is the time to (a) generate the large 1920px preview
// (the grid warmer only makes 320px thumbs) and (b) have the webview decode it.
// We fix both by pre-generating AND pre-decoding the previews for the photos
// just ahead/behind the one you're on, then HOLDING a reference to each decoded
// <img> so the webview doesn't evict the bitmap — that's what made going back a
// few shots re-blur. When Focus later sets the same asset URL, the decode is
// already warm and it appears instantly.
const LOUPE_RETAIN = 14; // ~a dozen 1920px previews kept warm (encoded bytes are small)
const loupeDecoded = new Map<string, HTMLImageElement>(); // path -> decoded image (LRU)
const loupeInflight = new Set<string>(); // paths currently being prefetched

/** Pre-generate + pre-decode the large Focus preview for `path`, and keep it
 *  warm. Cheap to call repeatedly (deduped + memoized). Images/RAW only. */
export function prefetchLoupe(path: string): void {
  const have = loupeDecoded.get(path);
  if (have) {
    // Mark most-recently-used so a backtrack keeps it alive.
    loupeDecoded.delete(path);
    loupeDecoded.set(path, have);
    return;
  }
  if (loupeInflight.has(path)) return;
  loupeInflight.add(path);
  // loupe_src goes through the shared cap/dedup queue (separate key from grid).
  enqueue(`loupe:${path}`, () => api.loupeSrc(path))
    .then((url) => {
      if (!url) return;
      const img = new Image();
      img.decoding = "async";
      img.src = url;
      // decode() forces the bitmap to be ready now, not on first paint.
      img.decode?.().catch(() => {});
      loupeDecoded.set(path, img);
      while (loupeDecoded.size > LOUPE_RETAIN) {
        const oldest = loupeDecoded.keys().next().value as string;
        loupeDecoded.delete(oldest);
      }
    })
    .finally(() => loupeInflight.delete(path));
}

/** Shared queue/dedup/cap machinery. `fetchFsPath` resolves to a filesystem path
 *  the backend produced; we convert it to an asset URL and memoize it. */
function enqueue(key: string, fetchFsPath: () => Promise<string>): Promise<string | null> {
  const cached = memo.get(key);
  if (cached) {
    retain(cached); // keep recently-shown thumbs warm even on a pure cache hit
    return Promise.resolve(cached);
  }

  const existing = pending.get(key);
  if (existing) {
    // Already queued/in-flight — bump it to the front (it's wanted again, now).
    const i = queue.findIndex((q) => q.key === key);
    if (i >= 0) {
      const [it] = queue.splice(i, 1);
      queue.push(it);
    }
    return existing;
  }

  const myGen = generation;
  const promise = new Promise<string | null>((resolve) => {
    const run = () => {
      if (myGen !== generation) {
        pending.delete(key);
        resolve(null);
        pump();
        return;
      }
      inflight++;
      fetchFsPath()
        .then((fsPath) => {
          const url = api.fileSrc(fsPath);
          memo.set(key, url);
          if (myGen === generation) {
            retain(url);
            resolve(url);
          } else {
            resolve(null);
          }
        })
        .catch(() => resolve(null))
        .finally(() => {
          inflight--;
          pending.delete(key);
          pump();
        });
    };
    queue.push({ key, run });
    pump();
  });

  pending.set(key, promise);
  return promise;
}

export function loadThumb(path: string, size: number): Promise<string | null> {
  return enqueue(`${path}@${size}`, () => api.thumbnail(path, size));
}
export function cancelThumb(path: string, size: number): void {
  cancel(`${path}@${size}`);
}

/** Cached video poster frame (bundled ffmpeg), through the same capped queue. */
export function loadVideoPoster(path: string): Promise<string | null> {
  return enqueue(`vid:${path}`, () => api.videoPoster(path));
}
export function cancelVideoPoster(path: string): void {
  cancel(`vid:${path}`);
}
