<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api } from "$lib/api";
  import { settings } from "$lib/settings.svelte";
  import { activity } from "$lib/activity.svelte";
  import { resetThumbs, prefetchLoupe, loaderStats } from "$lib/thumbnail-loader";
  import {
    LABELS,
    LABEL_BY_DIGIT,
    LABEL_VAR,
    type MediaItem,
    type TreeDir,
    type LibraryInfo,
    type TrashItem,
  } from "$lib/types";
  import TreeNode from "$lib/components/TreeNode.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import Loupe from "$lib/components/Loupe.svelte";
  import VirtualGrid from "$lib/components/VirtualGrid.svelte";
  import SectionedGrid from "$lib/components/SectionedGrid.svelte";
  import VirtualStrip from "$lib/components/VirtualStrip.svelte";
  import DetailsView from "$lib/components/DetailsView.svelte";
  import ContextMenu, { type MenuEntry } from "$lib/components/ContextMenu.svelte";
  import TrashPanel from "$lib/components/TrashPanel.svelte";
  import ActivityBar from "$lib/components/ActivityBar.svelte";

  type FlagFilter = "all" | "pick" | "reject" | "unflagged";
  type ViewMode = "grid" | "details" | "loupe";

  // Decode thumbnails at (roughly) the size they're DISPLAYED at, not a fixed
  // 320px. At the smallest grid a 320px thumb is ~6× the pixels actually shown —
  // wasted decode + memory. Snapping the request to a few tiers (so dragging the
  // zoom slider doesn't spawn dozens of cache variants) keeps the cached files,
  // the decoded bitmaps and the transfer all proportional to what's on screen.
  // Capped at 2 so a HiDPI panel doesn't quadruple memory.
  const DPR = typeof window !== "undefined" ? Math.min(window.devicePixelRatio || 1, 2) : 1;
  function tierFor(cssPx: number): number {
    const t = cssPx * DPR;
    if (t <= 200) return 192;
    if (t <= 340) return 320;
    return 480;
  }
  // Long edge of the full Focus preview (matches the backend LOUPE_MAX). Used by
  // "Prepare folder" to pre-generate every shot's big preview, not just the thumb.
  const LOUPE_MAX = 1920;
  // How many shots ahead/behind the active one to keep warm in Focus view.
  const PREFETCH_AHEAD = 3;
  const PREFETCH_BEHIND = 2;

  let drives = $state<TreeDir[]>([]);
  let currentDir = $state<string | null>(null);
  let items = $state<MediaItem[]>([]);
  let loading = $state(false);
  let writable = $state(true);

  let activeIndex = $state(0);
  let selected = $state<Set<string>>(new Set());

  let minRating = $state(0);
  let labelFilter = $state<string | null>(null);
  let flagFilter = $state<FlagFilter>("all");
  let tagFilter = $state<string | null>(null);
  let allTags = $state<[string, number][]>([]);
  let tagInput = $state("");

  // How many of the secondary (popover) filters are active — shown as a badge.
  let activeFilterCount = $derived(
    (minRating > 0 ? 1 : 0) + (labelFilter ? 1 : 0) + (tagFilter ? 1 : 0),
  );

  let dimLevel = $state(0); // 0 normal · 1 dim panels · 2 lights out
  let settingsOpen = $state(false);
  let filtersOpen = $state(false);
  let libInfo = $state<LibraryInfo | null>(null);
  let trashOpen = $state(false);
  let trashItems = $state<TrashItem[]>([]);
  // Bumped by the tree's ↻ button to make expanded folders recount their badges.
  let countsGen = $state(0);
  let gridComp = $state<{ scrollToIndex: (i: number, center?: boolean) => void } | null>(null);
  let loupeComp = $state<{ togglePlay: () => void; seekBy: (d: number) => void } | null>(null);

  const HOLD_MS = 850;
  let holdMs = $state(0);
  let holdRAF = 0;

  const basename = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  let viewMode = $derived(settings.s.viewMode as ViewMode);

  // Folder-grouped, human-numeric path order (IMG_2 < IMG_10, and each
  // subfolder's shots stay together instead of interleaving by bare filename —
  // that interleaving was the "random order" on recursive folder loads).
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

  // Real capture timestamps (path → Unix secs), filled lazily after a folder
  // loads so folder-open stays instant. Falls back to file mtime until/unless a
  // file's EXIF/creation_time is known.
  let captureMap = $state<Record<string, number>>({});
  const captureOf = (it: MediaItem) => captureMap[it.path] ?? it.mtime;

  // Grouping that needs real capture dates (the date-based sections); folder/type
  // group on the path/kind we already have, so they cost nothing extra.
  const DATE_GROUPS = new Set(["year", "month", "week"]);
  const TYPE_ORDER: Record<string, number> = { image: 0, raw: 1, video: 2, other: 3 };
  const TYPE_LABEL: Record<string, string> = {
    image: "Photos",
    raw: "RAW",
    video: "Video",
    other: "Other",
  };
  const parentOf = (p: string) => p.replace(/[\\/][^\\/]*$/, "");
  const parentName = (p: string) => parentOf(p).split(/[\\/]/).filter(Boolean).pop() ?? "/";

  // Section helpers for the grouped grid (folder · type · year · month · week).
  // Dates are UTC to match how capture timestamps are stored. Week = calendar
  // week-of-month (days 1–7 = Week 1, 8–14 = Week 2, …).
  function sectionKey(it: MediaItem): string {
    const g = settings.s.groupBy;
    if (g === "folder") return parentOf(it.path);
    if (g === "type") return it.kind;
    const d = new Date(captureOf(it) * 1000);
    if (g === "year") return `${d.getUTCFullYear()}`;
    const base = `${d.getUTCFullYear()}-${d.getUTCMonth()}`;
    if (g === "week") return `${base}-${Math.floor((d.getUTCDate() - 1) / 7)}`;
    return base; // month
  }
  function sectionLabel(it: MediaItem): string {
    const g = settings.s.groupBy;
    if (g === "folder") return parentName(it.path);
    if (g === "type") return TYPE_LABEL[it.kind] ?? it.kind;
    const d = new Date(captureOf(it) * 1000);
    if (g === "year") return `${d.getUTCFullYear()}`;
    const mon = d.toLocaleString(undefined, { month: "long", year: "numeric", timeZone: "UTC" });
    if (g === "week") return `${mon} · Week ${Math.floor((d.getUTCDate() - 1) / 7) + 1}`;
    return mon;
  }

  // type → rating/label/flag/tag filters → sort, in one pass. Grouping by month
  // implies sorting by capture date (that's the order the sections need).
  let view = $derived.by(() => {
    let arr = items;
    const tf = settings.s.typeFilter;
    if (tf !== "all") arr = arr.filter((i) => i.kind === tf);
    if (minRating > 0) arr = arr.filter((i) => i.rating >= minRating);
    if (labelFilter) arr = arr.filter((i) => i.label === labelFilter);
    if (tagFilter) arr = arr.filter((i) => i.tags.includes(tagFilter!));
    if (flagFilter === "reject") arr = arr.filter((i) => i.flag === "reject");
    else if (flagFilter === "pick") arr = arr.filter((i) => i.flag === "pick");
    else if (flagFilter === "unflagged") arr = arr.filter((i) => !i.flag);

    const g = settings.s.groupBy;
    const dir = settings.s.sortDir === "asc" ? 1 : -1;
    // Date groupings imply a capture-date order (that's the order their sections
    // need); folder/type keep their groups contiguous via a direction-independent
    // primary key, then order within each group by the chosen sort.
    const by = DATE_GROUPS.has(g) ? "capture" : settings.s.sortBy;
    return [...arr].sort((a, b) => {
      if (g === "folder") {
        const p = collator.compare(parentOf(a.path), parentOf(b.path));
        if (p !== 0) return p;
      } else if (g === "type") {
        const p = (TYPE_ORDER[a.kind] ?? 9) - (TYPE_ORDER[b.kind] ?? 9);
        if (p !== 0) return p;
      }
      let c = 0;
      if (by === "capture") c = captureOf(a) - captureOf(b);
      else if (by === "date") c = a.mtime - b.mtime;
      else if (by === "size") c = a.size - b.size;
      else if (by === "type") c = collator.compare(a.kind, b.kind);
      // "name" (and every tie) resolves to folder-grouped numeric path order.
      if (c === 0) c = collator.compare(a.path, b.path);
      return c * dir;
    });
  });

  // Capture-date sections over the (capture-sorted) view, for the grouped grid.
  let sections = $derived.by(() => {
    const out: { label: string; count: number }[] = [];
    let key = "";
    for (const it of view) {
      const k = sectionKey(it);
      if (k !== key) {
        out.push({ label: sectionLabel(it), count: 0 });
        key = k;
      }
      out[out.length - 1].count++;
    }
    return out;
  });
  let grouped = $derived(settings.s.groupBy !== "none" && viewMode === "grid");

  let active = $derived(view.length ? view[Math.min(activeIndex, view.length - 1)] : null);
  let rejectedCount = $derived(items.filter((i) => i.flag === "reject").length);
  let pickCount = $derived(items.filter((i) => i.flag === "pick").length);
  let stripCell = $derived(Math.max(64, settings.s.filmstripSize - 24));
  // Thumbnail decode sizes, matched to how big the cells are actually drawn.
  let gridThumbTier = $derived(tierFor(settings.s.gridSize));
  let stripThumbTier = $derived(tierFor(stripCell));

  $effect(() => {
    if (activeIndex > view.length - 1) activeIndex = Math.max(0, view.length - 1);
  });

  onMount(async () => {
    await settings.init();
    try {
      drives = await api.listDrives();
    } catch {
      drives = [];
    }
    try {
      libInfo = await api.libraryInfo();
    } catch {
      /* */
    }
    // Reopen the last folder AND land on the last photo we were looking at.
    if (settings.s.lastDir)
      openFolder(settings.s.lastDir, { selectPath: settings.s.lastActivePath });
  });

  // Heartbeat: log heap + loader caches every 20s so the logfile shows whether
  // memory climbs while scrolling a folder (not just across switches). In an
  // $effect (not the async onMount) so the interval is cleaned up correctly.
  $effect(() => {
    const beat = setInterval(() => {
      if (currentDir) logMem(`tick ${basename(currentDir)}`);
    }, 20000);
    return () => clearInterval(beat);
  });

  function rootForDir(dir: string): string {
    const d = drives.find((dr) => dir.toLowerCase().startsWith(dr.path.toLowerCase()));
    if (d) return d.path;
    const m = dir.match(/^[A-Za-z]:[\\/]/);
    return m ? m[0] : dir;
  }

  async function refreshTags() {
    try {
      allTags = await api.listTags();
    } catch {
      allTags = [];
    }
  }

  // Diagnostic memory probe → the on-disk logfile (UI MEM …). Lets us confirm the
  // JS heap + loader caches stay FLAT across folder switches instead of climbing
  // (the signature of the old "progressively worse" leak). `performance.memory`
  // is the renderer JS heap; watch msedgewebview2.exe in Task Manager for the
  // decoded-image memory, which Chromium manages off-heap.
  function logMem(tag: string) {
    try {
      const mem = (performance as unknown as { memory?: { usedJSHeapSize: number; jsHeapSizeLimit: number } })
        .memory;
      const s = loaderStats();
      const heap = mem
        ? `heapMB=${Math.round(mem.usedJSHeapSize / 1048576)}/${Math.round(mem.jsHeapSizeLimit / 1048576)}`
        : "heap=n/a";
      api.logEvent(
        `MEM ${tag} ${heap} memo=${s.memo} loupe=${s.loupe} pending=${s.pending} queue=${s.queue} inflight=${s.inflight}`,
      );
    } catch {
      /* diagnostics only — never throw */
    }
  }

  // Recompute the left-pane folder counts (they're cached and never auto-stale,
  // so this is the manual "I added/removed files" refresh).
  let recounting = $state(false);
  async function refreshCounts() {
    if (recounting) return;
    recounting = true;
    try {
      await api.clearFolderCounts();
      countsGen++;
    } finally {
      // Brief spin so the click feels acknowledged even when it's instant.
      setTimeout(() => (recounting = false), 400);
    }
  }

  async function openFolder(
    dir: string,
    opts: { selectPath?: string | null; selectIndex?: number } = {},
  ) {
    currentDir = dir;
    loading = true;
    resetThumbs();
    selected = new Set();
    captureMap = {};
    capturesDir = null;
    try {
      libInfo = await api.setLibraryRoot(rootForDir(dir));
      items = await api.listFolderMedia(dir, settings.s.includeSub);
      writable = await api.folderWritable(dir);
    } catch (e) {
      items = [];
      console.error(e);
    }
    // Land on the requested photo (restore on launch) or index (stay put after a
    // delete), else the top.
    let idx = 0;
    if (opts.selectPath) {
      const found = view.findIndex((i) => i.path === opts.selectPath);
      if (found >= 0) idx = found;
    } else if (opts.selectIndex != null) {
      idx = Math.max(0, Math.min(opts.selectIndex, view.length - 1));
    }
    activeIndex = idx;
    if (view.length) selected = new Set([view[idx].path]);
    loading = false;
    settings.set({ lastDir: dir });
    // Let the grid mount, then bring the restored/next photo into view.
    setTimeout(scrollActive, 80);
    // Warm thumbnails in the order they're shown (top-down), but only after the
    // visible cells have had a head start — the on-screen lazy loads grab the
    // disk first, then the warmer trickles the rest in. Guard against a folder
    // switch landing during the delay.
    const order = view.map((i) => i.path);
    const tier = gridThumbTier;
    setTimeout(() => {
      if (currentDir === dir) api.warmThumbnails(order, tier);
    }, 500);
    logMem(`open ${basename(dir)} n=${items.length}`);
    refreshTags();
    // Index real capture dates in the background — only when a date-driven view
    // needs them (sort-by-capture or month grouping). Cached after the first pass.
    maybeFetchCaptures();
  }

  /** Whether the current view depends on real capture dates. */
  let needCaptures = $derived(DATE_GROUPS.has(settings.s.groupBy) || settings.s.sortBy === "capture");

  let capturesDir: string | null = null;
  async function fetchCaptures(dir: string, paths: string[]) {
    if (!paths.length) return;
    capturesDir = dir;
    try {
      const res = await api.captureDates(dir, paths);
      if (currentDir !== dir) return;
      const m: Record<string, number> = {};
      for (const r of res) m[r.path] = r.captured;
      captureMap = m;
    } catch {
      capturesDir = null; // allow a retry
    }
  }
  function maybeFetchCaptures() {
    if (!currentDir || !needCaptures) return;
    if (capturesDir === currentDir) return; // already fetched for this folder
    fetchCaptures(
      currentDir,
      items.map((i) => i.path),
    );
  }

  async function openFolderPicker() {
    const picked = await api.pickFolder();
    if (picked) {
      if (!drives.length) {
        try {
          drives = await api.listDrives();
        } catch {
          /* */
        }
      }
      openFolder(picked);
    }
  }

  async function toggleSub() {
    await settings.set({ includeSub: !settings.s.includeSub });
    if (currentDir) await openFolder(currentDir);
  }

  function setView(v: ViewMode) {
    settings.set({ viewMode: v });
  }

  function targets(): MediaItem[] {
    if (selected.size > 1) return items.filter((i) => selected.has(i.path));
    return active ? [active] : [];
  }

  function scrollActive() {
    gridComp?.scrollToIndex(activeIndex);
  }

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  function rememberActive() {
    const a = view[activeIndex];
    if (!a) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => settings.set({ lastActivePath: a.path }), 400);
  }

  function setActiveTo(i: number) {
    activeIndex = Math.max(0, Math.min(i, view.length - 1));
    const a = view[activeIndex];
    if (a) selected = new Set([a.path]);
    scrollActive();
    rememberActive();
  }

  function move(delta: number) {
    setActiveTo(activeIndex + delta);
  }

  // ── Focus-view preview prefetch ────────────────────────────────────────────
  // Keep the shots just ahead/behind the active one decoded and warm, biased in
  // the direction of travel, so ←/→ in Focus is instant and short backtracks
  // don't re-blur. Videos are skipped (their poster is already warmed elsewhere).
  let lastPrefetchIndex = 0;
  function prefetchAroundActive() {
    if (viewMode !== "loupe" || !view.length) return;
    const dir = activeIndex >= lastPrefetchIndex ? 1 : -1;
    lastPrefetchIndex = activeIndex;
    const tryAt = (i: number) => {
      const it = view[i];
      if (it && (it.kind === "image" || it.kind === "raw")) prefetchLoupe(it.path);
    };
    for (let k = 1; k <= PREFETCH_AHEAD; k++) tryAt(activeIndex + dir * k);
    for (let k = 1; k <= PREFETCH_BEHIND; k++) tryAt(activeIndex - dir * k);
  }
  // Fire whenever the active shot or the view changes while in Focus.
  $effect(() => {
    activeIndex;
    viewMode;
    view;
    prefetchAroundActive();
  });

  // Restore grid position when returning from Focus: bring the shot you were
  // looking at back into the middle of the grid, instead of snapping to the top
  // (which happened because the grid component remounts at scroll 0).
  let prevViewMode: ViewMode = "grid";
  $effect(() => {
    const vm = viewMode;
    if (vm === "loupe" && prevViewMode !== "loupe") {
      // Entering Focus: abandon background warming so the big preview generation
      // and (especially) video playback get the USB SSD's read bandwidth instead
      // of stuttering behind the warmer.
      api.cancelWarm();
    }
    if (vm !== "loupe" && prevViewMode === "loupe") {
      const i = activeIndex;
      requestAnimationFrame(() => gridComp?.scrollToIndex(i, true));
    }
    prevViewMode = vm;
  });

  // ── Prepare folder: pre-cache full previews for the whole folder up front ──
  // The grid warmer only makes small thumbnails; this generates every shot's big
  // Focus preview (and video posters) so a culling pass through the folder has
  // zero blur. Runs on the backend's bounded pool; safe to keep working meanwhile.
  let preparing = $state(false);
  let prepared = $state(false);
  let prepDone = $state(0);
  let prepTotal = $state(0);
  let prepEta = $state("");
  let prepPct = $derived(prepTotal ? Math.round((prepDone / prepTotal) * 100) : 0);
  async function prepareFolder() {
    if (!currentDir || preparing || !view.length) return;
    preparing = true;
    prepared = false;
    const dir = currentDir;
    // Focus previews are the big (1920px) renders; the small grid thumbs are
    // already warmed on folder-open. We chunk the work so the button can show
    // real progress + a time estimate instead of an opaque spinner.
    const paths = view.filter((i) => i.kind !== "other").map((i) => i.path);
    prepTotal = paths.length;
    prepDone = 0;
    prepEta = "";
    const t0 = performance.now();
    const CHUNK = 16;
    try {
      for (let i = 0; i < paths.length; i += CHUNK) {
        if (currentDir !== dir) break; // folder switched — abandon
        await api.warmThumbnails(paths.slice(i, i + CHUNK), LOUPE_MAX);
        prepDone = Math.min(paths.length, i + CHUNK);
        const elapsed = performance.now() - t0;
        const remain = (elapsed / prepDone) * (paths.length - prepDone);
        prepEta = remain > 1500 ? `~${Math.ceil(remain / 1000)}s` : "almost done";
        // Mirror into the global activity chip (visible from any view).
        activity.local("prepare", "Preparing full-size previews", prepDone, prepTotal);
      }
    } finally {
      preparing = false;
      activity.end("prepare");
      // Only flash "ready" if we're still on the same folder we prepared.
      if (currentDir === dir) {
        prepared = true;
        setTimeout(() => (prepared = false), 2500);
      }
    }
  }

  function rate(r: number) {
    const ts = targets();
    if (ts.length === 1) {
      ts[0].rating = ts[0].rating === r ? 0 : r;
      api.setRating(ts[0].path, ts[0].rating).catch(() => {});
    } else if (ts.length > 1) {
      for (const it of ts) it.rating = r;
      api.setRatingMany(ts.map((i) => i.path), r).catch(() => {});
    }
  }
  function label(key: string) {
    const ts = targets();
    if (ts.length === 1) {
      ts[0].label = ts[0].label === key ? null : key;
      api.setLabel(ts[0].path, ts[0].label).catch(() => {});
    } else if (ts.length > 1) {
      for (const it of ts) it.label = key;
      api.setLabelMany(ts.map((i) => i.path), key).catch(() => {});
    }
  }
  function flag(f: "pick" | "reject") {
    const ts = targets();
    if (ts.length === 1) {
      ts[0].flag = ts[0].flag === f ? null : f;
      api.setFlag(ts[0].path, ts[0].flag).catch(() => {});
    } else if (ts.length > 1) {
      for (const it of ts) it.flag = f;
      api.setFlagMany(ts.map((i) => i.path), f).catch(() => {});
    }
  }
  function unset() {
    const ts = targets();
    if (!ts.length) return;
    for (const it of ts) {
      it.rating = 0;
      it.label = null;
      it.flag = null;
    }
    const paths = ts.map((i) => i.path);
    if (ts.length === 1) {
      api.setRating(paths[0], 0).catch(() => {});
      api.setLabel(paths[0], null).catch(() => {});
      api.setFlag(paths[0], null).catch(() => {});
    } else {
      api.setRatingMany(paths, 0).catch(() => {});
      api.setLabelMany(paths, null).catch(() => {});
      api.setFlagMany(paths, null).catch(() => {});
    }
  }

  // ── tags ──────────────────────────────────────────────────────────────────
  async function addTagToTargets() {
    const tag = tagInput.trim();
    const ts = targets();
    if (!tag || !ts.length) return;
    for (const it of ts) if (!it.tags.includes(tag)) it.tags = [...it.tags, tag];
    tagInput = "";
    await api.addTag(ts.map((i) => i.path), tag).catch(() => {});
    refreshTags();
  }
  async function removeTagFromActive(tag: string) {
    if (!active) return;
    active.tags = active.tags.filter((t) => t !== tag);
    await api.removeTag([active.path], tag).catch(() => {});
    refreshTags();
  }

  function selectAllFiltered() {
    selected = new Set(view.map((i) => i.path));
  }
  function rejectSelected() {
    const sel = items.filter((i) => selected.has(i.path));
    if (!sel.length) return;
    for (const it of sel) it.flag = "reject";
    api.setFlagMany(sel.map((i) => i.path), "reject").catch(() => {});
  }

  function gridCellClick(e: MouseEvent, i: number) {
    const it = view[i];
    if (!it) return;
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected);
      if (next.has(it.path)) next.delete(it.path);
      else next.add(it.path);
      selected = next;
      activeIndex = i;
    } else {
      setActiveTo(i);
    }
  }

  // ── right-click context menu (replaces the webview's native menu) ─────────
  const isMac =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Macintosh");
  const revealLabel = isMac ? "Reveal in Finder" : "Show in Explorer";
  let menu = $state<{ x: number; y: number; entries: MenuEntry[] } | null>(null);

  async function copyPath(p: string) {
    try {
      await navigator.clipboard.writeText(p);
    } catch {
      /* clipboard unavailable — ignore */
    }
  }

  function mediaMenuEntries(ctx: MediaItem, ts: MediaItem[]): MenuEntry[] {
    const sfx = ts.length > 1 ? ` (${ts.length})` : "";
    const allPick = ts.length > 0 && ts.every((i) => i.flag === "pick");
    const allReject = ts.length > 0 && ts.every((i) => i.flag === "reject");
    return [
      { label: "Previous", icon: "←", disabled: activeIndex <= 0, action: () => move(-1) },
      { label: "Next", icon: "→", disabled: activeIndex >= view.length - 1, action: () => move(1) },
      { separator: true },
      {
        label: viewMode === "loupe" ? "Back to grid" : "Open in Focus",
        icon: "▣",
        action: () => setView(viewMode === "loupe" ? "grid" : "loupe"),
      },
      {
        label: ctx.kind === "video" ? "Open in system player" : "Open in default app",
        icon: "▶",
        action: () => api.openExternal(ctx.path),
      },
      { label: revealLabel, icon: "⤴", action: () => api.reveal(ctx.path) },
      { separator: true },
      { label: (allPick ? "Clear pick" : "Pick") + sfx, icon: "✓", on: allPick, action: () => flag("pick") },
      {
        label: (allReject ? "Clear reject" : "Reject") + sfx,
        icon: "✕",
        danger: !allReject,
        on: allReject,
        action: () => flag("reject"),
      },
      { label: "Clear rating & marks" + sfx, icon: "⟲", action: () => unset() },
      { separator: true },
      {
        label: "Export as JPEG…" + sfx,
        icon: "⇩",
        disabled: !ts.some((i) => i.kind === "image" || i.kind === "raw"),
        action: () => exportTargets(),
      },
      { label: "Copy file path", icon: "⧉", action: () => copyPath(ctx.path) },
    ];
  }

  function openContextMenu(e: MouseEvent, ctx: MediaItem, i: number) {
    e.preventDefault();
    // Focus the right-clicked item unless it's already in a multi-selection.
    if (!(selected.size > 1 && selected.has(ctx.path))) setActiveTo(i);
    else activeIndex = i;
    menu = { x: e.clientX, y: e.clientY, entries: mediaMenuEntries(ctx, targets()) };
  }

  /** Suppress the webview's native menu everywhere except real text inputs. */
  function onGlobalContextMenu(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
    e.preventDefault();
  }

  // ── long-press delete (no modal, no toast) ──────────────────────────────
  function startHold() {
    if (rejectedCount === 0 || !writable) return;
    const t0 = performance.now();
    const tick = () => {
      holdMs = performance.now() - t0;
      if (holdMs >= HOLD_MS) {
        holdMs = 0;
        executeDelete();
      } else {
        holdRAF = requestAnimationFrame(tick);
      }
    };
    holdRAF = requestAnimationFrame(tick);
  }
  function endHold() {
    cancelAnimationFrame(holdRAF);
    holdMs = 0;
  }
  async function executeDelete() {
    const paths = await api.listRejected();
    if (!paths.length) return;
    // "folder" → the active drive's _FoxCull/recycle (recoverable in-app Trash);
    // "recycle" → the OS Recycle Bin / Trash.
    await api.disposeRejected(paths, settings.s.deleteMode);
    // Stay where we were — after the rejected shots vanish, the same index lands
    // on the next surviving photo, not back at the top of the folder.
    if (currentDir) await openFolder(currentDir, { selectIndex: activeIndex });
  }

  // ── panel resizing ──────────────────────────────────────────────────────
  function startTreeResize(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startW = settings.s.treeWidth;
    const move = (ev: PointerEvent) => {
      settings.s.treeWidth = Math.max(170, Math.min(560, startW + (ev.clientX - startX)));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      settings.set({ treeWidth: settings.s.treeWidth });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  function startStripResize(e: PointerEvent) {
    e.preventDefault();
    const right = settings.s.filmstripPos === "right";
    const start = right ? e.clientX : e.clientY;
    const startS = settings.s.filmstripSize;
    const move = (ev: PointerEvent) => {
      const d = right ? start - ev.clientX : start - ev.clientY;
      settings.s.filmstripSize = Math.max(84, Math.min(520, startS + d));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      settings.set({ filmstripSize: settings.s.filmstripSize });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  // ── full-screen mode (F): just the photo, everything else gone ───────────
  let fullscreen = $state(false);
  let fsPrevView: ViewMode = "grid";
  async function toggleFullscreen() {
    fullscreen = !fullscreen;
    try {
      await getCurrentWindow().setFullscreen(fullscreen);
    } catch {
      // wayland/odd WMs can refuse — the chrome still hides, which is most of it
    }
    if (fullscreen) {
      fsPrevView = viewMode;
      if (active) setView("loupe");
    } else {
      setView(fsPrevView);
    }
  }

  // ── export (RAW → camera-rendered JPEG; images copied through) ───────────
  async function exportTargets() {
    const ts = targets().filter((i) => i.kind === "image" || i.kind === "raw");
    if (!ts.length) {
      activity.error("export-result", "Nothing to export (photos and RAW only)");
      return;
    }
    const dest = await api.pickFolder();
    if (!dest) return;
    try {
      const r = await api.exportJpegs(ts.map((i) => i.path), dest);
      if (r.failed.length) {
        activity.error(
          "export-result",
          `Export: ${r.failed.length} of ${ts.length} failed — ${r.errors[0] ?? ""}`,
        );
      }
      // Show the result where the files are: open the destination folder.
      api.openExternal(r.dest);
    } catch (e) {
      activity.error("export-result", `Export failed (${e})`);
    }
  }

  // ── in-app Trash (per-drive recycle folder) ──────────────────────────────
  async function openTrash() {
    try {
      trashItems = await api.listTrash();
    } catch {
      trashItems = [];
    }
    trashOpen = true;
  }
  async function restoreFromTrash(stored: string[]) {
    await api.restoreTrash(stored);
    trashItems = await api.listTrash();
    // A restored file may belong to the open folder — refresh it.
    if (currentDir) await openFolder(currentDir, { selectIndex: activeIndex });
  }
  async function purgeFromTrash(stored: string[]) {
    await api.purgeTrash(stored);
    trashItems = await api.listTrash();
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
    // Video playback keys (Focus mode, active clip): Space toggles play/pause,
    // Shift+←/→ scrubs the clip. Plain ←/→ still move between items (below).
    if (viewMode === "loupe" && active?.kind === "video" && loupeComp) {
      if (e.key === " " || e.code === "Space") { loupeComp.togglePlay(); e.preventDefault(); return; }
      if (e.shiftKey && e.key === "ArrowRight") { loupeComp.seekBy(5); e.preventDefault(); return; }
      if (e.shiftKey && e.key === "ArrowLeft") { loupeComp.seekBy(-5); e.preventDefault(); return; }
    }
    if (e.key === "ArrowRight" || e.key === "ArrowDown") { move(1); e.preventDefault(); return; }
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") { move(-1); e.preventDefault(); return; }
    if (e.key === "Enter") { setView(viewMode === "loupe" ? "grid" : "loupe"); e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (fullscreen) toggleFullscreen();
      else if (dimLevel > 0) dimLevel = 0;
      else if (viewMode === "loupe") setView("grid");
      else selected = active ? new Set([active.path]) : new Set();
      return;
    }
    const k = e.key.toLowerCase();
    if (k === "f") { toggleFullscreen(); return; }
    if (k === "l") { dimLevel = (dimLevel + 1) % 3; return; }
    if (k === "g") { setView("grid"); return; }
    if (k === "d") { setView("details"); return; }
    if (e.key >= "1" && e.key <= "5") { rate(Number(e.key)); return; }
    if (e.key === "`") { rate(0); return; }
    if (e.key in LABEL_BY_DIGIT) { label(LABEL_BY_DIGIT[e.key]); return; }
    if (k === "x") { flag("reject"); return; }
    if (k === "p") { flag("pick"); return; }
    if (k === "u") { unset(); return; }
  }

  // Mouse back/forward buttons → a simple Focus⇄grid toggle (no history stack):
  // Forward (button 4) on a selected shot opens Focus; Back (button 3) from Focus
  // returns to the grid (which scroll-restores to that shot). preventDefault stops
  // the webview trying to navigate its history and blanking the single-page app.
  function onmouseup(e: MouseEvent) {
    if (e.button === 3) {
      if (viewMode === "loupe") { setView("grid"); e.preventDefault(); }
    } else if (e.button === 4) {
      if (viewMode !== "loupe" && active) { setView("loupe"); e.preventDefault(); }
    }
  }
</script>

<svelte:window {onkeydown} {onmouseup} oncontextmenu={onGlobalContextMenu} />

{#snippet gridCell(item: MediaItem, i: number)}
  <button
    class="cell"
    class:active={i === activeIndex}
    class:selected={selected.has(item.path)}
    class:reject={item.flag === "reject"}
    onclick={(e) => gridCellClick(e, i)}
    ondblclick={() => { setActiveTo(i); setView("loupe"); }}
    oncontextmenu={(e) => openContextMenu(e, item, i)}
  >
    <Thumb {item} size={gridThumbTier} />
    <span class="ov">
      {#if item.label}<span class="lbl-dot" style="background:var({LABEL_VAR[item.label]})"></span>{/if}
      {#if item.flag === "reject"}<span class="fl x">✕</span>{/if}
      {#if item.flag === "pick"}<span class="fl pick">✓</span>{/if}
      {#if item.rating > 0}<span class="stars">{"★".repeat(item.rating)}</span>{/if}
      {#if item.tags.length}<span class="tagdot" title={item.tags.join(", ")}>🏷</span>{/if}
    </span>
  </button>
{/snippet}

{#snippet stripCellSnip(item: MediaItem, i: number)}
  <button
    class="scell"
    class:active={i === activeIndex}
    class:reject={item.flag === "reject"}
    onclick={() => setActiveTo(i)}
    ondblclick={() => { setActiveTo(i); setView("loupe"); }}
    oncontextmenu={(e) => openContextMenu(e, item, i)}
    title={item.name}
  >
    <Thumb {item} size={stripThumbTier} />
    {#if item.label}<span class="s-lbl" style="background:var({LABEL_VAR[item.label]})"></span>{/if}
    {#if item.rating > 0}<span class="s-stars">{"★".repeat(item.rating)}</span>{/if}
    {#if item.flag === "reject"}<span class="s-x">✕</span>{/if}
    {#if item.flag === "pick"}<span class="s-pick">✓</span>{/if}
  </button>
{/snippet}

<div class="app" data-dim={dimLevel} class:fs={fullscreen}>
  <!-- ░ left: drives + folder tree ░ -->
  <aside class="tree" style="width:{settings.s.treeWidth}px">
    <div class="tree-head">
      <span class="brand">🦊 fox-cull</span>
      <div class="tree-actions">
        <button
          class="ico sm"
          class:spin={recounting}
          onclick={refreshCounts}
          title="Recount folders (the counts are cached — refresh after adding or removing files)"
          aria-label="Recount folders"
        >↻</button>
        <button class="btn sm" onclick={openFolderPicker} title="Jump to a folder">Open…</button>
      </div>
    </div>
    <ActivityBar />
    <div class="tree-body">
      {#if drives.length}
        {#each drives as d (d.path)}
          <TreeNode node={d} {currentDir} onselect={openFolder} {countsGen} />
        {/each}
      {:else}
        <p class="hint">No drives detected.</p>
      {/if}
    </div>
  </aside>

  <div class="vsplit" role="separator" tabindex="-1" onpointerdown={startTreeResize}></div>

  <!-- ░ center ░ -->
  <main class="center">
    {#if !writable && currentDir}
      <div class="banner">Read-only location — rating works; the delete sweep is disabled here.</div>
    {/if}

    <!-- top bar -->
    <div class="bar">
      <!-- view mode -->
      <div class="seg modes" title="View (G grid · D details · Enter focus)">
        <button class="chip" class:on={viewMode === "grid"} onclick={() => setView("grid")}>▦ Grid</button>
        <button class="chip" class:on={viewMode === "details"} onclick={() => setView("details")}>≣ Details</button>
        <button class="chip" class:on={viewMode === "loupe"} onclick={() => setView("loupe")}>▣ Focus</button>
      </div>

      <span class="div"></span>

      <!-- sort + date grouping -->
      <div class="grp">
        <select class="sel" title="Sort order" bind:value={settings.s.sortBy} onchange={() => { settings.set({ sortBy: settings.s.sortBy }); maybeFetchCaptures(); }}>
          <option value="name">Name</option>
          <option value="date">Date (modified)</option>
          <option value="capture">Capture date</option>
          <option value="type">Type</option>
          <option value="size">Size</option>
        </select>
        <button class="ico" title="Sort direction" onclick={() => settings.set({ sortDir: settings.s.sortDir === "asc" ? "desc" : "asc" })}>
          {settings.s.sortDir === "asc" ? "↑" : "↓"}
        </button>
        <select class="sel" title="Split the grid into sections" bind:value={settings.s.groupBy} onchange={() => { settings.set({ groupBy: settings.s.groupBy }); maybeFetchCaptures(); }}>
          <option value="none">No groups</option>
          <option value="folder">Group: folder</option>
          <option value="type">Group: type</option>
          <option value="year">Group: year</option>
          <option value="month">Group: month</option>
          <option value="week">Group: week</option>
        </select>
      </div>

      <span class="div"></span>

      <!-- type + flag (primary culling filters) -->
      <div class="seg">
        {#each [["all", "All"], ["image", "Photos"], ["video", "Video"], ["raw", "RAW"]] as [val, lbl]}
          <button class="chip" class:on={settings.s.typeFilter === val} onclick={() => settings.set({ typeFilter: val as typeof settings.s.typeFilter })}>{lbl}</button>
        {/each}
      </div>

      <div class="seg flags">
        <button class="chip" class:on={flagFilter === "all"} onclick={() => (flagFilter = "all")}>All</button>
        <button class="chip pick" class:on={flagFilter === "pick"} onclick={() => (flagFilter = "pick")}>Picks</button>
        <button class="chip rej" class:on={flagFilter === "reject"} onclick={() => (flagFilter = "reject")}>Rejected</button>
        <button class="chip" class:on={flagFilter === "unflagged"} onclick={() => (flagFilter = "unflagged")}>Unflagged</button>
      </div>

      <!-- consolidated secondary filters (rating · label · tag · scope) -->
      <div class="grp filterwrap">
        <button class="chip" class:on={filtersOpen || activeFilterCount > 0} onclick={() => (filtersOpen = !filtersOpen)} title="Rating, label, tag & scope filters">
          ⛃ Filters{activeFilterCount ? ` ·${activeFilterCount}` : ""}
        </button>
        {#if filtersOpen}
          <div class="filtermenu">
            <div class="fm-row">
              <span class="fm-lbl">Rating</span>
              <div class="seg">
                {#each [1, 2, 3, 4, 5] as n}
                  <button class="starf" class:on={minRating >= n} onclick={() => (minRating = minRating === n ? 0 : n)} title="{n}+ stars">★</button>
                {/each}
                {#if minRating > 0}<button class="fm-clr" onclick={() => (minRating = 0)}>clear</button>{/if}
              </div>
            </div>
            <div class="fm-row">
              <span class="fm-lbl">Label</span>
              <div class="seg">
                <button class="dot any" class:on={labelFilter === null} onclick={() => (labelFilter = null)} title="Any label">∅</button>
                {#each LABELS as l}
                  <button class="dot" class:on={labelFilter === l.key} style="background:var({l.varName})" title={l.name} aria-label={l.name} onclick={() => (labelFilter = labelFilter === l.key ? null : l.key)}></button>
                {/each}
              </div>
            </div>
            <div class="fm-row col">
              <span class="fm-lbl">Tag</span>
              <div class="fm-tags">
                <button class="tagrow" class:on={tagFilter === null} onclick={() => (tagFilter = null)}>Any tag</button>
                {#if allTags.length}
                  {#each allTags as [t, n]}
                    <button class="tagrow" class:on={tagFilter === t} onclick={() => (tagFilter = t)}>
                      <span>{t}</span><span class="cnt">{n}</span>
                    </button>
                  {/each}
                {:else}
                  <p class="tagempty">No tags yet. Add one to the selected photo below.</p>
                {/if}
              </div>
            </div>
            <div class="fm-row">
              <span class="fm-lbl">Scope</span>
              <button class="chip" class:on={settings.s.includeSub} onclick={toggleSub} title="Include photos from subfolders">⊞ Include subfolders</button>
            </div>
          </div>
        {/if}
      </div>

      {#if viewMode === "grid"}
        <span class="div"></span>
        <div class="grp zoom" title="Thumbnail size">
          <span class="mini">▦</span>
          <input type="range" min="110" max="360" bind:value={settings.s.gridSize} onchange={() => settings.set({ gridSize: settings.s.gridSize })} />
        </div>
      {/if}

      <div class="spacer"></div>

      <!-- actions (top-right) -->
      <button
        class="btn sm prep"
        class:on={preparing || prepared}
        onclick={prepareFolder}
        disabled={!view.length || preparing}
        title="Pre-render full-size Focus previews for this whole folder, so flipping through it in Focus view has zero loading blur. (Grid thumbnails are already pre-cached when a folder opens.)"
      >
        {#if preparing}<span class="prep-fill" style="width:{prepPct}%"></span>{/if}
        <span class="prep-lbl">
          {#if preparing}⏳ {prepPct}%{prepEta ? ` · ${prepEta}` : ""}{:else if prepared}✓ Ready{:else}⚡ Prepare{/if}
        </span>
      </button>
      <button class="btn sm" onclick={selectAllFiltered} disabled={!view.length} title="Select all in view">Select all{view.length ? ` (${view.length})` : ""}</button>
      <button class="btn sm danger" onclick={rejectSelected} disabled={selected.size === 0} title="Flag the selection as reject">
        Reject{selected.size > 1 ? ` ${selected.size}` : ""}
      </button>
      <button
        class="btn sm danger hold"
        disabled={!writable || rejectedCount === 0}
        onpointerdown={startHold}
        onpointerup={endHold}
        onpointerleave={endHold}
        onpointercancel={endHold}
        title="Hold to delete all {rejectedCount} rejected"
      >
        <span class="hold-fill" style="width:{(holdMs / HOLD_MS) * 100}%"></span>
        <span class="hold-lbl">🗑 Delete{rejectedCount ? ` ${rejectedCount}` : ""} <em>(hold)</em></span>
      </button>
      <button class="btn sm" onclick={openTrash} title="View deleted items — restore or remove permanently">♻ Trash</button>
      <button class="ico gear" class:on={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)} title="Settings">⚙</button>
    </div>

    <!-- settings popover -->
    {#if settingsOpen}
      <div class="pop">
        <div class="row"><span>Theme</span>
          <div class="seg">
            <button class="chip" class:on={settings.s.theme === "dark"} onclick={() => settings.set({ theme: "dark" })}>Dark</button>
            <button class="chip" class:on={settings.s.theme === "warm"} onclick={() => settings.set({ theme: "warm" })} title="Low-blue amber chrome for long sessions in a dim, warmly-lit room — the photo stage stays neutral">Warm</button>
            <button class="chip" class:on={settings.s.theme === "light"} onclick={() => settings.set({ theme: "light" })}>Light</button>
          </div>
        </div>
        <div class="row"><span>Filmstrip</span>
          <div class="seg">
            {#each [["bottom", "Bottom"], ["right", "Right"], ["hidden", "Off"]] as [v, l]}
              <button class="chip" class:on={settings.s.filmstripPos === v} onclick={() => settings.set({ filmstripPos: v as typeof settings.s.filmstripPos })}>{l}</button>
            {/each}
          </div>
        </div>
        <div class="row"><span>On delete</span>
          <div class="seg">
            <button class="chip" class:on={settings.s.deleteMode === "folder"} onclick={() => settings.set({ deleteMode: "folder" })} title="Move to this drive's _FoxCull recycle folder — recoverable in the in-app Trash">In-app Trash</button>
            <button class="chip" class:on={settings.s.deleteMode === "recycle"} onclick={() => settings.set({ deleteMode: "recycle" })} title="Send to the operating system's Recycle Bin / Trash">System Recycle Bin</button>
          </div>
        </div>
        <div class="row"><span>Trash</span>
          <button class="btn sm" onclick={() => { settingsOpen = false; openTrash(); }}>🗑 Open Trash…</button>
        </div>
        <div class="row"><span>Library</span>
          {#if libInfo}
            <button class="btn sm" onclick={() => libInfo && api.reveal(libInfo.catalog)} title="Show the library folder in your file manager">Reveal</button>
          {/if}
        </div>
        {#if libInfo}
          <div class="row sub">
            <span class="path" title={libInfo.dir}>{libInfo.dir}</span>
            <span class="tag">{libInfo.on_drive ? "on drive" : "app-data (read-only mount)"}</span>
          </div>
        {/if}
        <div class="row hintrow">Each drive keeps its own catalog, preview cache &amp; recycle in a <code>_FoxCull</code> folder. Press <kbd>F</kbd> full screen · <kbd>L</kbd> dim · <kbd>G</kbd> grid · <kbd>D</kbd> details.</div>
      </div>
    {/if}

    {#if trashOpen}
      <TrashPanel
        items={trashItems}
        onclose={() => (trashOpen = false)}
        onrestore={restoreFromTrash}
        onpurge={purgeFromTrash}
      />
    {/if}

    <!-- body: viewport (+ optional right filmstrip) -->
    <div class="body">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="viewport"
        class:lit={dimLevel > 0}
        oncontextmenu={(e) => {
          if (viewMode === "loupe" && active) openContextMenu(e, active, activeIndex);
        }}
      >
        {#if loading}
          <div class="welcome"><p>Scanning {currentDir ? basename(currentDir) : ""}…</p></div>
        {:else if !currentDir}
          <div class="welcome">
            <h1>🦊 fox-cull</h1>
            <p>Pick a folder on the left to start culling. Browse-in-place — nothing is imported or changed.</p>
          </div>
        {:else if view.length === 0}
          <div class="welcome"><p>Nothing here matches the current filters.</p></div>
        {:else if viewMode === "loupe"}
          <Loupe item={active} bind:this={loupeComp} />
        {:else if viewMode === "details"}
          <DetailsView
            items={view}
            {activeIndex}
            {selected}
            onrowclick={gridCellClick}
            onrowdblclick={(i) => { setActiveTo(i); setView("loupe"); }}
          />
        {:else if grouped}
          <SectionedGrid
            items={view}
            groups={sections}
            {activeIndex}
            cellMin={settings.s.gridSize}
            bind:this={gridComp}
            cell={gridCell}
          />
        {:else}
          <VirtualGrid items={view} {activeIndex} cellMin={settings.s.gridSize} bind:this={gridComp} cell={gridCell} />
        {/if}
      </div>

      {#if settings.s.filmstripPos === "right" && view.length}
        <div class="vsplit" role="separator" tabindex="-1" onpointerdown={startStripResize}></div>
        <aside class="rstrip" style="width:{settings.s.filmstripSize}px">
          <VirtualStrip items={view} {activeIndex} orientation="v" cellSize={stripCell} cell={stripCellSnip} />
        </aside>
      {/if}
    </div>

    <!-- active-item info bar -->
    {#if active}
      <div class="info">
        <span class="name" title={active.path}>{active.name}</span>
        <span class="meta">{active.kind} · {activeIndex + 1}/{view.length}</span>
        <div class="rate">
          {#each [1, 2, 3, 4, 5] as n}
            <button class="star" class:on={active.rating >= n} onclick={() => rate(n)}>★</button>
          {/each}
        </div>
        {#each LABELS as l}
          <button class="dot sm" class:on={active.label === l.key} style="background:var({l.varName})" title={l.name} aria-label={l.name} onclick={() => label(l.key)}></button>
        {/each}
        <button class="btn sm" class:on={active.flag === "pick"} onclick={() => flag("pick")}>Pick</button>
        <button class="btn sm danger" class:on={active.flag === "reject"} onclick={() => flag("reject")}>Reject</button>

        <!-- tags -->
        <div class="tags">
          {#each active.tags as t}
            <span class="tag">{t}<button class="tagx" onclick={() => removeTagFromActive(t)} aria-label="Remove tag">×</button></span>
          {/each}
          <input
            class="taginput"
            placeholder="+ tag"
            bind:value={tagInput}
            onkeydown={(e) => { if (e.key === "Enter") addTagToTargets(); }}
          />
        </div>

        <span class="spacer"></span>
        <button class="ico" title="Reveal in file manager" onclick={() => active && api.reveal(active.path)}>⤴</button>
        <span class="counts">✓ {pickCount} · ✕ {rejectedCount}</span>
      </div>
    {/if}

    <!-- bottom filmstrip -->
    {#if settings.s.filmstripPos === "bottom" && view.length}
      <div class="hsplit" role="separator" tabindex="-1" onpointerdown={startStripResize} title="Drag to resize"><span class="grip"></span></div>
      <div class="bstrip" style="height:{settings.s.filmstripSize}px">
        <VirtualStrip items={view} {activeIndex} orientation="h" cellSize={stripCell} cell={stripCellSnip} />
      </div>
    {/if}
  </main>

  <!-- dim / lights-out scrim: darkens all chrome, the photo viewport stays lit -->
  {#if dimLevel > 0}
    <button class="scrim" aria-label="Exit dim mode" onclick={() => (dimLevel = 0)}></button>
  {/if}

  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} entries={menu.entries} onclose={() => (menu = null)} />
  {/if}
</div>

<style>
  .app { display: flex; height: 100vh; overflow: hidden; }
  /* Full-screen mode (F): nothing but the photo stage — every panel, bar and
     strip disappears and the viewport fills the (OS-fullscreened) window. */
  .app.fs .tree,
  .app.fs .vsplit,
  .app.fs .hsplit,
  .app.fs .bar,
  .app.fs .banner,
  .app.fs .info,
  .app.fs .bstrip,
  .app.fs .rstrip,
  .app.fs .pop { display: none; }
  .tree { display: flex; flex-direction: column; background: var(--bg-panel); border-right: 1px solid var(--border); flex: 0 0 auto; min-width: 0; }
  .tree-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 9px 10px; border-bottom: 1px solid var(--border); }
  .tree-actions { display: flex; align-items: center; gap: 6px; }
  .ico.sm { width: 26px; height: 26px; font-size: 13px; }
  .ico.spin { animation: spin 0.5s linear; color: var(--accent); border-color: var(--accent); }
  @keyframes spin { to { transform: rotate(360deg); } }
  .brand { font-weight: 700; }
  .tree-body { overflow-y: auto; padding: 6px; flex: 1; }
  .hint { padding: 10px; color: var(--text-faint); font-size: 12.5px; }

  .vsplit { flex: 0 0 5px; cursor: col-resize; background: transparent; }
  .vsplit:hover { background: color-mix(in srgb, var(--accent) 40%, transparent); }
  .hsplit { flex: 0 0 8px; cursor: row-resize; display: flex; align-items: center; justify-content: center; background: var(--bg-panel); border-top: 1px solid var(--border); }
  .hsplit .grip { width: 46px; height: 3px; border-radius: 3px; background: var(--text-faint); opacity: 0.4; }
  .hsplit:hover { background: color-mix(in srgb, var(--accent) 22%, var(--bg-panel)); }
  .hsplit:hover .grip { opacity: 0.9; background: var(--accent); }

  .center { display: flex; flex-direction: column; flex: 1; min-width: 0; height: 100vh; }

  .bar { position: relative; display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-bottom: 1px solid var(--border); background: var(--bg-panel); flex-wrap: wrap; }
  .grp { display: flex; align-items: center; gap: 4px; }
  .seg { display: flex; align-items: center; gap: 3px; }
  .seg.flags { gap: 2px; }
  .seg.modes { gap: 2px; padding: 2px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 8px; }
  .spacer { flex: 1; }
  .sel { background: var(--bg-elev); color: var(--text); border: 1px solid var(--border); border-radius: 7px; padding: 4px 6px; font-size: 12.5px; }
  .ico { width: 28px; height: 28px; border-radius: 7px; border: 1px solid var(--border); background: var(--bg-elev); font-size: 14px; line-height: 1; }
  .ico:hover { background: var(--bg-hover); }
  .ico.on { border-color: var(--accent); color: var(--accent); }
  .chip { padding: 4px 9px; border-radius: 6px; font-size: 12px; color: var(--text-dim); border: 1px solid transparent; white-space: nowrap; }
  .chip:hover { background: var(--bg-hover); }
  .chip.on { background: var(--accent); color: var(--accent-on); }
  .chip.rej.on { background: var(--reject); border-color: var(--reject); }
  .chip.pick.on { background: var(--pick); border-color: var(--pick); }
  .starf { font-size: 14px; color: var(--text-faint); padding: 0 1px; }
  .starf.on { color: var(--star); }
  .dot { width: 14px; height: 14px; border-radius: 3px; border: 1px solid rgba(0,0,0,0.25); opacity: 0.5; }
  .dot.any { background: var(--bg-elev); color: var(--text-faint); font-size: 10px; line-height: 12px; opacity: 1; }
  .dot.sm { width: 13px; height: 13px; }
  .dot.on { opacity: 1; outline: 2px solid var(--accent); outline-offset: 1px; }
  .zoom { gap: 6px; }
  .zoom .mini { color: var(--text-faint); font-size: 12px; }
  .zoom input { width: 90px; accent-color: var(--accent); }
  .btn.sm { padding: 5px 10px; border-radius: 7px; font-size: 12.5px; }
  .btn.sm.on { border-color: var(--accent); color: var(--accent); }
  .prep { position: relative; overflow: hidden; min-width: 96px; text-align: center; }
  .prep-fill { position: absolute; left: 0; top: 0; bottom: 0; background: color-mix(in srgb, var(--accent) 30%, transparent); transition: width 0.2s ease; }
  .prep-lbl { position: relative; z-index: 1; white-space: nowrap; }

  .div { flex: 0 0 auto; align-self: stretch; width: 1px; margin: 2px 4px; background: var(--border); }
  .filterwrap { position: relative; }
  .filtermenu { position: absolute; top: 34px; left: 0; z-index: 30; width: 290px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow); padding: 11px; display: flex; flex-direction: column; gap: 11px; }
  .fm-row { display: flex; align-items: center; gap: 10px; }
  .fm-row.col { flex-direction: column; align-items: stretch; gap: 5px; }
  .fm-lbl { flex: 0 0 46px; font-size: 12px; color: var(--text-dim); }
  .fm-tags { display: flex; flex-direction: column; gap: 2px; max-height: 200px; overflow-y: auto; }
  .fm-clr { font-size: 11px; color: var(--text-faint); padding: 0 4px; margin-left: 4px; }
  .fm-clr:hover { color: var(--text); }
  .tagrow { display: flex; justify-content: space-between; gap: 10px; width: 100%; text-align: left; padding: 6px 9px; border-radius: 6px; font-size: 12.5px; color: var(--text); }
  .tagrow:hover { background: var(--bg-hover); }
  .tagrow.on { background: var(--accent); color: var(--accent-on); }
  .tagrow .cnt { color: var(--text-faint); }
  .tagrow.on .cnt { color: var(--accent-on); }
  .tagempty { padding: 8px 9px; color: var(--text-faint); font-size: 12px; margin: 0; }

  .hold { position: relative; overflow: hidden; }
  .hold-fill { position: absolute; left: 0; top: 0; bottom: 0; background: color-mix(in srgb, var(--reject) 35%, transparent); }
  .hold-lbl { position: relative; z-index: 1; }
  .hold em { font-style: normal; opacity: 0.6; font-size: 11px; }

  .pop { position: absolute; right: 10px; top: 46px; z-index: 30; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow); padding: 12px; width: 340px; display: flex; flex-direction: column; gap: 10px; }
  .pop .row { display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: 13px; }
  .pop .row.sub { padding-left: 6px; flex-wrap: nowrap; }
  .pop .path { flex: 1; min-width: 0; color: var(--text-dim); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pop .row.sub .tag { flex: 0 0 auto; }
  /* Prose row — MUST be block, not flex: flex + space-between turns the text
     fragments around <code>/<kbd> into separate squeezed flex items and the
     whole sentence collapses into a one-word-per-line column. */
  .pop .row.hintrow { display: block; color: var(--text-faint); font-size: 12px; line-height: 1.7; }
  kbd { background: var(--bg-panel); border: 1px solid var(--border); border-radius: 4px; padding: 0 5px; font-size: 11px; }

  .body { flex: 1; display: flex; min-height: 0; }
  .viewport { flex: 1; min-width: 0; background: var(--viewport-bg); overflow: hidden; display: flex; flex-direction: column; }
  .viewport.lit { position: relative; z-index: 50; }
  .rstrip { flex: 0 0 auto; border-left: 1px solid var(--border); }
  .bstrip { flex: 0 0 auto; }

  .welcome { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--text-dim); text-align: center; padding: 24px; }
  .welcome h1 { font-size: 28px; margin: 0; }

  .cell { position: relative; width: 100%; height: 100%; border: 2px solid transparent; border-radius: 6px; overflow: hidden; padding: 0; background: var(--viewport-bg); }
  .cell.selected { border-color: var(--text-faint); }
  .cell.active { border-color: var(--accent); }
  .cell.reject :global(.media) { opacity: 0.35; }
  .ov { position: absolute; inset: 0; pointer-events: none; }
  .lbl-dot { position: absolute; top: 5px; right: 5px; width: 12px; height: 12px; border-radius: 3px; border: 1px solid rgba(0,0,0,0.4); }
  .fl { position: absolute; top: 4px; left: 6px; font-weight: 700; text-shadow: 0 1px 3px rgba(0,0,0,0.6); }
  .fl.x { color: var(--reject); }
  .fl.pick { color: var(--pick); }
  .stars { position: absolute; bottom: 4px; left: 6px; color: var(--star); font-size: 13px; text-shadow: 0 1px 3px rgba(0,0,0,0.6); }
  .tagdot { position: absolute; bottom: 4px; right: 6px; font-size: 11px; filter: drop-shadow(0 1px 2px rgba(0,0,0,0.6)); }

  .scell { position: relative; width: 100%; height: 100%; border: 2px solid transparent; border-radius: 5px; overflow: hidden; padding: 0; background: var(--viewport-bg); }
  .scell.active { border-color: var(--accent); }
  .scell.reject { opacity: 0.45; }
  .s-lbl { position: absolute; top: 3px; right: 3px; width: 10px; height: 10px; border-radius: 2px; }
  .s-stars { position: absolute; bottom: 2px; left: 3px; font-size: 10px; color: var(--star); text-shadow: 0 1px 2px rgba(0,0,0,0.6); }
  .s-x { position: absolute; top: 2px; left: 4px; color: var(--reject); font-weight: 700; text-shadow: 0 1px 2px rgba(0,0,0,0.6); }
  .s-pick { position: absolute; top: 2px; left: 4px; color: var(--pick); font-weight: 700; text-shadow: 0 1px 2px rgba(0,0,0,0.6); }

  .info { display: flex; align-items: center; gap: 10px; padding: 5px 12px; border-top: 1px solid var(--border); background: var(--bg-panel); }
  .info .name { font-weight: 600; max-width: 240px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .info .meta { color: var(--text-faint); font-size: 12px; }
  .info .counts { color: var(--text-faint); font-size: 12.5px; }
  .rate { display: flex; }
  .star { color: var(--text-faint); font-size: 16px; }
  .star.on { color: var(--star); }

  .tags { display: flex; align-items: center; gap: 5px; flex-wrap: nowrap; overflow: hidden; }
  .tag { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 11px; padding: 1px 4px 1px 8px; color: var(--text-dim); white-space: nowrap; }
  .tagx { font-size: 13px; line-height: 1; color: var(--text-faint); padding: 0 2px; }
  .tagx:hover { color: var(--reject); }
  .taginput { width: 70px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 11px; padding: 2px 8px; font-size: 11.5px; color: var(--text); }
  .taginput:focus { outline: none; border-color: var(--accent); width: 110px; }

  /* dim / lights-out scrim */
  .scrim { position: fixed; inset: 0; z-index: 40; border: none; padding: 0; cursor: pointer; background: rgba(0,0,0,0.55); transition: background 0.18s; }
  .app[data-dim="2"] .scrim { background: rgba(0,0,0,0.93); }
</style>
