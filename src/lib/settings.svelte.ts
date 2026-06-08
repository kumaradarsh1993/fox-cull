// Persisted app settings (theme, layout, sorting, delete behavior).
// Mirrors the wispr-fox settings-store pattern: a runes-powered class that
// loads once and writes through to tauri-plugin-store on every change.
import { Store } from "@tauri-apps/plugin-store";

export type Theme = "light" | "dark";
export type ViewMode = "grid" | "details" | "loupe";
export type FilmstripPos = "bottom" | "right" | "hidden";
export type SortBy = "name" | "date" | "capture" | "type" | "size";
export type SortDir = "asc" | "desc";
export type GroupBy = "none" | "month" | "week";
export type TypeFilter = "all" | "image" | "video" | "raw";
export type DeleteMode = "recycle" | "folder";

export interface AppSettings {
  theme: Theme;
  viewMode: ViewMode;
  filmstripPos: FilmstripPos;
  treeWidth: number;
  filmstripSize: number;
  gridSize: number;
  sortBy: SortBy;
  sortDir: SortDir;
  /** Section the grid by real capture date — off, by month, or by week. */
  groupBy: GroupBy;
  typeFilter: TypeFilter;
  includeSub: boolean;
  deleteMode: DeleteMode;
  rejectFolder: string | null;
  lastDir: string | null;
  lastActivePath: string | null;
}

const DEFAULTS: AppSettings = {
  theme: "dark",
  viewMode: "grid",
  filmstripPos: "bottom",
  treeWidth: 270,
  filmstripSize: 132,
  gridSize: 176,
  sortBy: "name",
  sortDir: "asc",
  groupBy: "none",
  typeFilter: "all",
  includeSub: true,
  deleteMode: "recycle",
  rejectFolder: null,
  lastDir: null,
  lastActivePath: null,
};

const FILE = "fox-cull-settings.json";
const KEY = "settings";

class Settings {
  s = $state<AppSettings>({ ...DEFAULTS });
  ready = $state(false);
  private store: Store | null = null;

  async init() {
    if (this.ready) return;
    try {
      this.store = await Store.load(FILE);
      const loaded = await this.store.get<AppSettings & { groupByMonth?: boolean }>(KEY);
      if (loaded) {
        // Migrate the old boolean month toggle to the new granularity field.
        if (loaded.groupBy === undefined && loaded.groupByMonth) loaded.groupBy = "month";
        this.s = { ...DEFAULTS, ...loaded };
      }
    } catch {
      // first run / store unavailable — defaults stand
    }
    this.ready = true;
  }

  async set(patch: Partial<AppSettings>) {
    Object.assign(this.s, patch);
    try {
      if (this.store) {
        await this.store.set(KEY, { ...this.s });
        await this.store.save();
      }
    } catch {
      // ignore persistence failures (settings still apply in-session)
    }
  }
}

export const settings = new Settings();
