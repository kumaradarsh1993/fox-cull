import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  TreeDir,
  MediaItem,
  TrashOutcome,
  TrashItem,
  LibraryInfo,
  FilmstripInfo,
  ExportOutcome,
} from "./types";

export const api = {
  /** Activate the per-drive library for `root` and return where it lives. */
  setLibraryRoot: (root: string) =>
    invoke<LibraryInfo>("set_library_root", { root }),
  listDrives: () => invoke<TreeDir[]>("list_drives"),
  listTree: (dir: string) => invoke<TreeDir[]>("list_tree", { dir }),
  /** Recursive media counts for the given folders (cached; left-pane badges). */
  folderCounts: (paths: string[], recompute = false) =>
    invoke<{ path: string; count: number }[]>("folder_counts", { paths, recompute }),
  /** Drop every cached folder count so the badges recompute. */
  clearFolderCounts: () => invoke<void>("clear_folder_counts").catch(() => {}),
  listFolderMedia: (dir: string, recursive: boolean) =>
    invoke<MediaItem[]>("list_folder_media", { dir, recursive }),
  thumbnail: (path: string, max: number) =>
    invoke<string>("thumbnail", { path, max }),
  /** Fire-and-forget: pre-warm the whole folder's thumbnails in parallel. */
  warmThumbnails: (paths: string[], max: number) =>
    invoke<void>("warm_thumbnails", { paths, max }).catch(() => {}),
  /** Abandon in-flight background warming (free the SSD for previews/playback). */
  cancelWarm: () => invoke<void>("cancel_warm").catch(() => {}),
  loupeSrc: (path: string) => invoke<string>("loupe_src", { path }),
  /** Cached poster frame (filesystem path) for a video, via bundled ffmpeg. */
  videoPoster: (path: string) => invoke<string>("video_poster", { path }),
  /** Tiled sprite of frames for decode-free scrubbing (built lazily, cached). */
  videoFilmstrip: (path: string) =>
    invoke<FilmstripInfo>("video_filmstrip", { path }),
  /** Real capture timestamps (EXIF/creation_time), cached; for sort + grouping. */
  captureDates: (dir: string, paths: string[]) =>
    invoke<{ path: string; captured: number }[]>("capture_dates", { dir, paths }),
  /** Convert a clip the webview can't decode into a cached H.264 preview. */
  videoProxy: (path: string) => invoke<string>("video_proxy", { path }),
  /** Path of an already-built H.264 proxy for the clip, or null. */
  videoProxyCached: (path: string) =>
    invoke<string | null>("video_proxy_cached", { path }).catch(() => null),
  /** Export files into `dest`: RAW → camera-rendered JPEG, images copied. */
  exportJpegs: (paths: string[], dest: string) =>
    invoke<ExportOutcome>("export_jpegs", { paths, dest }),
  getTrim: (path: string) => invoke<[number, number] | null>("get_trim", { path }),
  setTrim: (path: string, inS: number, outS: number) =>
    invoke<void>("set_trim", { path, in_s: inS, out_s: outS }).catch(() => {}),
  clearTrim: (path: string) => invoke<void>("clear_trim", { path }).catch(() => {}),
  trimVideo: (path: string, inS: number, outS: number) =>
    invoke<string>("trim_video", { path, in_s: inS, out_s: outS }),
  setRating: (path: string, rating: number) =>
    invoke<void>("set_rating", { path, rating }),
  setLabel: (path: string, label: string | null) =>
    invoke<void>("set_label", { path, label }),
  setFlag: (path: string, flag: string | null) =>
    invoke<void>("set_flag", { path, flag }),
  setRatingMany: (paths: string[], rating: number) =>
    invoke<void>("set_rating_many", { paths, rating }),
  setLabelMany: (paths: string[], label: string | null) =>
    invoke<void>("set_label_many", { paths, label }),
  setFlagMany: (paths: string[], flag: string | null) =>
    invoke<void>("set_flag_many", { paths, flag }),
  addTag: (paths: string[], tag: string) =>
    invoke<void>("add_tag", { paths, tag }),
  removeTag: (paths: string[], tag: string) =>
    invoke<void>("remove_tag", { paths, tag }),
  listTags: () => invoke<[string, number][]>("list_tags"),
  listRejected: () => invoke<string[]>("list_rejected"),
  disposeRejected: (paths: string[], mode: string) =>
    invoke<TrashOutcome>("dispose_rejected", { paths, mode }),
  /** In-app, per-drive Trash (folder-mode deletes). */
  listTrash: () => invoke<TrashItem[]>("list_trash"),
  restoreTrash: (stored: string[]) =>
    invoke<{ restored: number; failed: string[] }>("restore_trash", { stored }),
  purgeTrash: (stored: string[]) => invoke<number>("purge_trash", { stored }),
  libraryInfo: () => invoke<LibraryInfo>("library_info"),
  reveal: (path: string) => invoke<void>("reveal", { path }).catch(() => {}),
  openExternal: (path: string) =>
    invoke<void>("open_external", { path }).catch(() => {}),
  folderWritable: (dir: string) => invoke<boolean>("folder_writable", { dir }),
  logEvent: (msg: string) => invoke<void>("log_event", { msg }).catch(() => {}),

  /** Convert an absolute filesystem path into a webview-loadable asset URL. */
  fileSrc: (path: string) => convertFileSrc(path),

  pickFolder: async (): Promise<string | null> => {
    const r = await openDialog({ directory: true, multiple: false });
    return typeof r === "string" ? r : null;
  },
};
