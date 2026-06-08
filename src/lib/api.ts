import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { TreeDir, MediaItem, TrashOutcome, CatalogInfo } from "./types";

export const api = {
  setLibraryRoot: (root: string) => invoke<void>("set_library_root", { root }),
  listDrives: () => invoke<TreeDir[]>("list_drives"),
  listTree: (dir: string) => invoke<TreeDir[]>("list_tree", { dir }),
  listFolderMedia: (dir: string, recursive: boolean) =>
    invoke<MediaItem[]>("list_folder_media", { dir, recursive }),
  thumbnail: (path: string, max: number) =>
    invoke<string>("thumbnail", { path, max }),
  /** Fire-and-forget: pre-warm the whole folder's thumbnails in parallel. */
  warmThumbnails: (paths: string[], max: number) =>
    invoke<void>("warm_thumbnails", { paths, max }).catch(() => {}),
  loupeSrc: (path: string) => invoke<string>("loupe_src", { path }),
  /** Cached poster frame (filesystem path) for a video, via bundled ffmpeg. */
  videoPoster: (path: string) => invoke<string>("video_poster", { path }),
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
  disposeRejected: (paths: string[], mode: string, dest: string | null) =>
    invoke<TrashOutcome>("dispose_rejected", { paths, mode, dest }),
  catalogInfo: () => invoke<CatalogInfo>("catalog_info"),
  setCatalogDir: (dir: string) => invoke<string>("set_catalog_dir", { dir }),
  resetCatalogDir: () => invoke<string>("reset_catalog_dir"),
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
