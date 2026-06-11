export interface TreeDir {
  name: string;
  path: string;
  has_children: boolean;
}

export interface MediaItem {
  name: string;
  path: string;
  rel: string;
  kind: "image" | "raw" | "video" | "other";
  ext: string;
  mtime: number;
  size: number;
  rating: number;
  label: string | null;
  flag: "pick" | "reject" | null;
  tags: string[];
}

export interface TrashOutcome {
  deleted: number;
  failed: string[];
  errors: string[];
}

export interface LibraryInfo {
  /** Drive/volume root the library belongs to (catalog keys are relative to it). */
  root: string;
  /** The active library folder: `<drive>/_FoxCull` or an app-data fallback. */
  dir: string;
  catalog: string;
  recycle: string;
  /** True if on the drive itself; false = app-data fallback (read-only mount). */
  on_drive: boolean;
  /** Whether the drive root is writable (proxy for "can delete here"). */
  writable: boolean;
}

export interface TrashItem {
  stored: string;
  orig: string;
  /** Absolute path of the file inside the recycle folder (for its thumbnail). */
  path: string;
  name: string;
  kind: "image" | "raw" | "video" | "other";
  ext: string;
  deleted_at: number;
}

/** A tiled sprite of frames for decode-free video scrubbing (Tier 2). */
export interface FilmstripInfo {
  src: string;
  cols: number;
  rows: number;
  count: number;
  tile_w: number;
  tile_h: number;
  duration: number;
}

/** Result of a JPEG export run (RAW → camera-rendered JPEG; images copied). */
export interface ExportOutcome {
  exported: number;
  copied: number;
  skipped: number;
  failed: string[];
  errors: string[];
  dest: string;
}

export interface Filter {
  minRating: number;
  label: string | null;
  flag: "pick" | "reject" | "unflagged" | null;
}

/** Color labels. Digits chosen to match the user's Lightroom muscle memory
 *  (8/9/0 = red/green/yellow), with 6/7 as bonus blue/purple. */
export interface LabelDef {
  key: string;
  digit: string;
  varName: string;
  name: string;
}

export const LABELS: LabelDef[] = [
  { key: "blue", digit: "6", varName: "--label-blue", name: "Blue" },
  { key: "purple", digit: "7", varName: "--label-purple", name: "Purple" },
  { key: "red", digit: "8", varName: "--label-red", name: "Red" },
  { key: "green", digit: "9", varName: "--label-green", name: "Green" },
  { key: "yellow", digit: "0", varName: "--label-yellow", name: "Yellow" },
];

export const LABEL_BY_DIGIT: Record<string, string> = Object.fromEntries(
  LABELS.map((l) => [l.digit, l.key]),
);

export const LABEL_VAR: Record<string, string> = Object.fromEntries(
  LABELS.map((l) => [l.key, l.varName]),
);
