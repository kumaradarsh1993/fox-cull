// Concurrency-capped thumbnail loader.
//
// - caps concurrent decodes (never floods Rust with hundreds of 12MP decodes)
// - memoizes resolved URLs (scroll-back is instant, zero IPC)
// - DE-DUPLICATES in-flight requests: if the same image is requested again
//   before the first decode finishes (grid cells re-mount during layout, and
//   the grid/filmstrip can ask for the same size), they share ONE decode
// - generation token abandons queued work for the old folder on a switch

import { api } from "./api";

const MAX_INFLIGHT = 6;
const memo = new Map<string, string>(); // key -> asset url (persists across folders)
const pending = new Map<string, Promise<string | null>>(); // key -> in-flight promise
const queue: Array<() => void> = [];
let inflight = 0;
let generation = 0;

function pump() {
  while (inflight < MAX_INFLIGHT && queue.length) {
    queue.shift()!();
  }
}

/** Abandon queued (not-yet-started) work — call when the folder changes. */
export function resetThumbs() {
  generation++;
  queue.length = 0;
  pending.clear();
  // Drop the warm decoded-preview window too — its images belong to the folder
  // we're leaving, and releasing the references lets the webview reclaim them.
  loupeDecoded.clear();
  loupeInflight.clear();
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
  if (cached) return Promise.resolve(cached);

  const existing = pending.get(key);
  if (existing) return existing; // already in flight — share it

  const myGen = generation;
  const promise = new Promise<string | null>((resolve) => {
    queue.push(() => {
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
          resolve(myGen === generation ? url : null);
        })
        .catch(() => resolve(null))
        .finally(() => {
          inflight--;
          pending.delete(key);
          pump();
        });
    });
    pump();
  });

  pending.set(key, promise);
  return promise;
}

export function loadThumb(path: string, size: number): Promise<string | null> {
  return enqueue(`${path}@${size}`, () => api.thumbnail(path, size));
}

/** Cached video poster frame (bundled ffmpeg), through the same capped queue. */
export function loadVideoPoster(path: string): Promise<string | null> {
  return enqueue(`vid:${path}`, () => api.videoPoster(path));
}
