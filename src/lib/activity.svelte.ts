// Background-activity tracker — the model behind the Lightroom-style progress
// chip in the left pane. Backend jobs (thumbnail warming, capture-date reads,
// exports, proxy transcodes, filmstrip builds) emit `activity` events; purely
// frontend-driven jobs (Prepare folder) report through `local()`. Finished
// jobs linger briefly so quick operations are still visible, then drop out.

import { listen } from "@tauri-apps/api/event";

export interface Job {
  id: string;
  label: string;
  done: number;
  /** 0 = indeterminate (spinner, no percentage). */
  total: number;
  state: "running" | "done" | "error";
  /** Last-update timestamp, for stable ordering. */
  ts: number;
}

const LINGER_DONE_MS = 1800;
const LINGER_ERROR_MS = 6000;

class ActivityStore {
  jobs = $state<Record<string, Job>>({});
  private started = false;
  private reapers = new Map<string, ReturnType<typeof setTimeout>>();

  list = $derived(Object.values(this.jobs).sort((a, b) => a.ts - b.ts));
  running = $derived(this.list.filter((j) => j.state === "running"));

  async init() {
    if (this.started) return;
    this.started = true;
    try {
      await listen<Omit<Job, "ts">>("activity", (e) => this.apply(e.payload));
    } catch {
      // not running inside Tauri (tests) — local jobs still work
    }
  }

  private apply(j: Omit<Job, "ts">) {
    this.jobs[j.id] = { ...j, ts: this.jobs[j.id]?.ts ?? Date.now() };
    const old = this.reapers.get(j.id);
    if (old) clearTimeout(old);
    if (j.state !== "running") {
      const wait = j.state === "error" ? LINGER_ERROR_MS : LINGER_DONE_MS;
      this.reapers.set(
        j.id,
        setTimeout(() => {
          if (this.jobs[j.id]?.state !== "running") delete this.jobs[j.id];
          this.reapers.delete(j.id);
        }, wait),
      );
    }
  }

  /** Report a frontend-driven job (e.g. Prepare folder). Call with done===total
   *  (or `end()`) to finish it. */
  local(id: string, label: string, done: number, total: number) {
    this.apply({ id, label, done, total, state: done >= total && total > 0 ? "done" : "running" });
  }

  end(id: string) {
    const j = this.jobs[id];
    if (j) this.apply({ ...j, state: "done" });
  }

  /** Surface a one-off failure message (lingers a few seconds, then clears). */
  error(id: string, label: string) {
    this.apply({ id, label, done: 0, total: 1, state: "error" });
  }
}

export const activity = new ActivityStore();
