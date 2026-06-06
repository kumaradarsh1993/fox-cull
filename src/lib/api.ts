import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { TreeDir, MediaItem, TrashOutcome } from "./types";

export const api = {
  setLibraryRoot: (root: string) => invoke<void>("set_library_root", { root }),
  listDrives: () => invoke<TreeDir[]>("list_drives"),
  listTree: (dir: string) => invoke<TreeDir[]>("list_tree", { dir }),
  listFolderMedia: (dir: string, recursive: boolean) =>
    invoke<MediaItem[]>("list_folder_media", { dir, recursive }),
  thumbnail: (path: string, max: number) =>
    invoke<string>("thumbnail", { path, max }),
  loupeSrc: (path: string) => invoke<string>("loupe_src", { path }),
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
  listRejected: () => invoke<string[]>("list_rejected"),
  disposeRejected: (paths: string[], mode: string, dest: string | null) =>
    invoke<TrashOutcome>("dispose_rejected", { paths, mode, dest }),
  folderWritable: (dir: string) => invoke<boolean>("folder_writable", { dir }),
  logEvent: (msg: string) => invoke<void>("log_event", { msg }).catch(() => {}),

  /** Convert an absolute filesystem path into a webview-loadable asset URL. */
  fileSrc: (path: string) => convertFileSrc(path),

  pickFolder: async (): Promise<string | null> => {
    const r = await openDialog({ directory: true, multiple: false });
    return typeof r === "string" ? r : null;
  },
};
