// Viewport-prioritized, cancellable, MEMORY-DISCIPLINED thumbnail loader.
//
// Design notes (after auditing the "progressively worse / not responding" bug):
//
// The freeze was NOT decode speed — folder scans are 0-3 ms and thumbnail files
// are generated once and disk-cached. It was MEMORY: an earlier version held a
// large LRU of decoded <img> bitmaps "warm" (up to 700 grid thumbs + a dozen
// 1920px previews ≈ 350 MB). Scrolling a big folder accumulated that fast and the
// WebView process ballooned until it thrashed. Holding decoded bitmaps in JS
// fights the browser's own image-cache eviction — exactly the wrong move for a
// virtualized grid of hundreds of images.
//
// The disciplined approach:
//  - hold (almost) NO decoded grid bitmaps. Virtualization keeps only the visible
//    ~2 screens of <img> alive; the browser decodes/evicts them as it sees fit.
//    Scroll-back is still fast because the asset URL stays in the WebView's own
//    resource cache (a memory-cache hit, no IPC, quick re-decode of a small thumb).
//  - memoize resolved URLs (bounded LRU) so we never re-invoke Rust for a thumb we
//    already resolved, but DON'T pin its pixels.
//  - cap concurrent decodes; serve the CURRENT viewport first (LIFO); a cell that
//    scrolls away before its decode starts CANCELS its request.
//  - keep ONLY a tiny bounded set of decoded Focus previews warm (those are big,
//    1920px, and re-decoding them IS visible as a blur — the grid is not).
//  - generation token abandons queued work for the old folder on a switch.

import { api } from "./api";

const MAX_INFLIGHT = 6; // parallel decodes — enough to fill a viewport, gentle on the USB SSD
const MEMO_CAP = 4000; // bound the URL cache so a long session can't grow it unbounded

const memo = new Map<string, string>(); // key -> asset url (LRU, bounded)
const pending = new Map<string, Promise<string | null>>(); // key -> in-flight promise
type QItem = { key: string; run: () => void };
let queue: QItem[] = []; // served LIFO (newest request = current viewport first)
let inflight = 0;
let generation = 0;

function memoGet(key: string): string | undefined {
  const v = memo.get(key);
  if (v !== undefined) {
    memo.delete(key);
    memo.set(key, v); // refresh recency
  }
  return v;
}
function memoSet(key: string, url: string) {
  memo.set(key, url);
  if (memo.size > MEMO_CAP) {
    const oldest = memo.keys().next().value as string;
    memo.delete(oldest);
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
  // Release the warm Focus previews from the folder we're leaving.
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

/** Lightweight stats for the diagnostic memory log. */
export function loaderStats() {
  return {
    memo: memo.size,
    loupe: loupeDecoded.size,
    pending: pending.size,
    queue: queue.length,
    inflight,
  };
}

// ── loupe (Focus-view) preview prefetch — the ONLY place we pin bitmaps ──────
//
// Focus previews are large (1920px ≈ 11 MB decoded each) and re-decoding one IS
// visible as a blur, so we keep a SMALL bounded set warm: the shots just
// ahead/behind the one you're on. Kept tiny (6) so the held memory is bounded
// (~66 MB max) and released entirely on a folder switch.
const LOUPE_RETAIN = 6;
const loupeDecoded = new Map<string, HTMLImageElement>(); // path -> decoded image (LRU)
const loupeInflight = new Set<string>(); // paths currently being prefetched

/** Pre-generate + pre-decode the large Focus preview for `path`, and keep it
 *  warm. Cheap to call repeatedly (deduped + memoized). Images/RAW only. */
export function prefetchLoupe(path: string): void {
  const have = loupeDecoded.get(path);
  if (have) {
    loupeDecoded.delete(path);
    loupeDecoded.set(path, have); // mark most-recently-used
    return;
  }
  if (loupeInflight.has(path)) return;
  loupeInflight.add(path);
  enqueue(`loupe:${path}`, () => api.loupeSrc(path))
    .then((url) => {
      if (!url) return;
      const img = new Image();
      img.decoding = "async";
      img.src = url;
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
  const cached = memoGet(key);
  if (cached) return Promise.resolve(cached);

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
          memoSet(key, url);
          resolve(myGen === generation ? url : null);
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
