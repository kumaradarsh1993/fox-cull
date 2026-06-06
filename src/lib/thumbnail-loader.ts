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
}

export function loadThumb(path: string, size: number): Promise<string | null> {
  const key = `${path}@${size}`;

  const cached = memo.get(key);
  if (cached) return Promise.resolve(cached);

  const existing = pending.get(key);
  if (existing) return existing; // someone is already decoding this exact thumb

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
      api
        .thumbnail(path, size)
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
