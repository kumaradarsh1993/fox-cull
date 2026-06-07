<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { settings } from "$lib/settings.svelte";
  import { resetThumbs } from "$lib/thumbnail-loader";
  import {
    LABELS,
    LABEL_BY_DIGIT,
    LABEL_VAR,
    type MediaItem,
    type TreeDir,
    type CatalogInfo,
  } from "$lib/types";
  import TreeNode from "$lib/components/TreeNode.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import Loupe from "$lib/components/Loupe.svelte";
  import VirtualGrid from "$lib/components/VirtualGrid.svelte";
  import VirtualStrip from "$lib/components/VirtualStrip.svelte";
  import DetailsView from "$lib/components/DetailsView.svelte";

  type FlagFilter = "all" | "pick" | "reject" | "unflagged";
  type ViewMode = "grid" | "details" | "loupe";

  // Long edge we cache grid thumbnails at (matches <Thumb size>); used to warm.
  const THUMB_MAX = 320;

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

  let dimLevel = $state(0); // 0 normal · 1 dim panels · 2 lights out
  let settingsOpen = $state(false);
  let tagMenuOpen = $state(false);
  let catInfo = $state<CatalogInfo | null>(null);
  let gridComp = $state<{ scrollToIndex: (i: number) => void } | null>(null);

  const HOLD_MS = 850;
  let holdMs = $state(0);
  let holdRAF = 0;

  const basename = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  let viewMode = $derived(settings.s.viewMode as ViewMode);

  // type → rating/label/flag/tag filters → sort, in one pass.
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

    const dir = settings.s.sortDir === "asc" ? 1 : -1;
    const by = settings.s.sortBy;
    return [...arr].sort((a, b) => {
      let c = 0;
      if (by === "date") c = a.mtime - b.mtime;
      else if (by === "size") c = a.size - b.size;
      else if (by === "type") c = a.kind.localeCompare(b.kind);
      if (c === 0) c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
      return c * dir;
    });
  });

  let active = $derived(view.length ? view[Math.min(activeIndex, view.length - 1)] : null);
  let rejectedCount = $derived(items.filter((i) => i.flag === "reject").length);
  let pickCount = $derived(items.filter((i) => i.flag === "pick").length);
  let stripCell = $derived(Math.max(64, settings.s.filmstripSize - 24));

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
      catInfo = await api.catalogInfo();
    } catch {
      /* */
    }
    // Reopen the last folder AND land on the last photo we were looking at.
    if (settings.s.lastDir)
      openFolder(settings.s.lastDir, { selectPath: settings.s.lastActivePath });
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

  async function openFolder(
    dir: string,
    opts: { selectPath?: string | null; selectIndex?: number } = {},
  ) {
    currentDir = dir;
    loading = true;
    resetThumbs();
    selected = new Set();
    try {
      await api.setLibraryRoot(rootForDir(dir));
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
    setTimeout(() => {
      if (currentDir === dir) api.warmThumbnails(order, THUMB_MAX);
    }, 500);
    refreshTags();
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
  function pickTagFilter(t: string | null) {
    tagFilter = t;
    tagMenuOpen = false;
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
    let dest = settings.s.rejectFolder;
    // "folder" mode with no explicit folder → backend auto-targets a per-drive
    // "_FoxCull Recycle Bin" mirroring the structure. No prompt needed.
    await api.disposeRejected(
      paths,
      settings.s.deleteMode,
      settings.s.deleteMode === "folder" ? dest : null,
    );
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

  async function chooseRejectFolder() {
    const f = await api.pickFolder();
    if (f) await settings.set({ rejectFolder: f });
  }
  async function moveCatalog() {
    const d = await api.pickFolder();
    if (!d) return;
    try {
      await api.setCatalogDir(d);
      catInfo = await api.catalogInfo();
      if (currentDir) await openFolder(currentDir);
    } catch (e) {
      console.error(e);
    }
  }
  async function resetCatalog() {
    try {
      await api.resetCatalogDir();
      catInfo = await api.catalogInfo();
      if (currentDir) await openFolder(currentDir);
    } catch (e) {
      console.error(e);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") { move(1); e.preventDefault(); return; }
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") { move(-1); e.preventDefault(); return; }
    if (e.key === "Enter") { setView(viewMode === "loupe" ? "grid" : "loupe"); e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (dimLevel > 0) dimLevel = 0;
      else if (viewMode === "loupe") setView("grid");
      else selected = active ? new Set([active.path]) : new Set();
      return;
    }
    const k = e.key.toLowerCase();
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
</script>

<svelte:window {onkeydown} />

{#snippet gridCell(item: MediaItem, i: number)}
  <button
    class="cell"
    class:active={i === activeIndex}
    class:selected={selected.has(item.path)}
    class:reject={item.flag === "reject"}
    onclick={(e) => gridCellClick(e, i)}
    ondblclick={() => { setActiveTo(i); setView("loupe"); }}
  >
    <Thumb {item} size={320} />
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
    title={item.name}
  >
    <Thumb {item} size={320} />
    {#if item.label}<span class="s-lbl" style="background:var({LABEL_VAR[item.label]})"></span>{/if}
    {#if item.rating > 0}<span class="s-stars">{"★".repeat(item.rating)}</span>{/if}
    {#if item.flag === "reject"}<span class="s-x">✕</span>{/if}
    {#if item.flag === "pick"}<span class="s-pick">✓</span>{/if}
  </button>
{/snippet}

<div class="app" data-dim={dimLevel}>
  <!-- ░ left: drives + folder tree ░ -->
  <aside class="tree" style="width:{settings.s.treeWidth}px">
    <div class="tree-head">
      <span class="brand">🦊 fox-cull</span>
      <button class="btn sm" onclick={openFolderPicker} title="Jump to a folder">Open…</button>
    </div>
    <div class="tree-body">
      {#if drives.length}
        {#each drives as d (d.path)}
          <TreeNode node={d} {currentDir} onselect={openFolder} />
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

      <div class="grp">
        <select class="sel" bind:value={settings.s.sortBy} onchange={() => settings.set({ sortBy: settings.s.sortBy })}>
          <option value="name">Name</option>
          <option value="date">Date</option>
          <option value="type">Type</option>
          <option value="size">Size</option>
        </select>
        <button class="ico" title="Sort direction" onclick={() => settings.set({ sortDir: settings.s.sortDir === "asc" ? "desc" : "asc" })}>
          {settings.s.sortDir === "asc" ? "↑" : "↓"}
        </button>
      </div>

      <div class="seg">
        {#each [["all", "All"], ["image", "Photos"], ["video", "Video"], ["raw", "RAW"]] as [val, lbl]}
          <button class="chip" class:on={settings.s.typeFilter === val} onclick={() => settings.set({ typeFilter: val as typeof settings.s.typeFilter })}>{lbl}</button>
        {/each}
      </div>

      <div class="seg">
        {#each [1, 2, 3, 4, 5] as n}
          <button class="starf" class:on={minRating >= n} onclick={() => (minRating = minRating === n ? 0 : n)} title="Filter {n}+ stars">★</button>
        {/each}
      </div>

      <div class="seg flags">
        <button class="chip" class:on={flagFilter === "all"} onclick={() => (flagFilter = "all")}>All</button>
        <button class="chip pick" class:on={flagFilter === "pick"} onclick={() => (flagFilter = "pick")}>Picks</button>
        <button class="chip rej" class:on={flagFilter === "reject"} onclick={() => (flagFilter = "reject")}>Rejected</button>
        <button class="chip" class:on={flagFilter === "unflagged"} onclick={() => (flagFilter = "unflagged")}>Unflagged</button>
      </div>

      <div class="seg">
        <button class="dot any" class:on={labelFilter === null} onclick={() => (labelFilter = null)} title="Any label">∅</button>
        {#each LABELS as l}
          <button class="dot" class:on={labelFilter === l.key} style="background:var({l.varName})" title={l.name} aria-label={l.name} onclick={() => (labelFilter = labelFilter === l.key ? null : l.key)}></button>
        {/each}
      </div>

      <!-- tags filter -->
      <div class="grp tagwrap">
        <button class="chip" class:on={tagFilter !== null} onclick={() => (tagMenuOpen = !tagMenuOpen)} title="Filter by tag">
          🏷 {tagFilter ?? "Tags"}
        </button>
        {#if tagMenuOpen}
          <div class="tagmenu">
            <button class="tagrow" class:on={tagFilter === null} onclick={() => pickTagFilter(null)}>Any tag</button>
            {#if allTags.length}
              {#each allTags as [t, n]}
                <button class="tagrow" class:on={tagFilter === t} onclick={() => pickTagFilter(t)}>
                  <span>{t}</span><span class="cnt">{n}</span>
                </button>
              {/each}
            {:else}
              <p class="tagempty">No tags yet. Add one to the selected photo below.</p>
            {/if}
          </div>
        {/if}
      </div>

      <button class="chip" class:on={settings.s.includeSub} onclick={toggleSub} title="Include photos from subfolders">⊞ Sub</button>

      {#if viewMode === "grid"}
        <div class="grp zoom" title="Thumbnail size">
          <span class="mini">▦</span>
          <input type="range" min="110" max="360" bind:value={settings.s.gridSize} onchange={() => settings.set({ gridSize: settings.s.gridSize })} />
        </div>
      {/if}

      <div class="spacer"></div>

      <!-- actions (top-right) -->
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
      <button class="ico gear" class:on={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)} title="Settings">⚙</button>
    </div>

    <!-- settings popover -->
    {#if settingsOpen}
      <div class="pop">
        <div class="row"><span>Theme</span>
          <div class="seg">
            <button class="chip" class:on={settings.s.theme === "dark"} onclick={() => settings.set({ theme: "dark" })}>Dark</button>
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
            <button class="chip" class:on={settings.s.deleteMode === "recycle"} onclick={() => settings.set({ deleteMode: "recycle" })}>Recycle Bin</button>
            <button class="chip" class:on={settings.s.deleteMode === "folder"} onclick={() => settings.set({ deleteMode: "folder" })}>Move to folder</button>
          </div>
        </div>
        {#if settings.s.deleteMode === "folder"}
          <div class="row sub">
            <span class="path" title={settings.s.rejectFolder ?? ""}>{settings.s.rejectFolder ? basename(settings.s.rejectFolder) : "Auto: _FoxCull Recycle Bin (drive root)"}</span>
            <div class="seg">
              <button class="btn sm" onclick={chooseRejectFolder}>Choose…</button>
              {#if settings.s.rejectFolder}<button class="btn sm" onclick={() => settings.set({ rejectFolder: null })}>Auto</button>{/if}
            </div>
          </div>
        {/if}
        <div class="row"><span>Catalog</span>
          <div class="seg">
            <button class="btn sm" onclick={moveCatalog}>Move…</button>
            {#if catInfo && !catInfo.is_default}<button class="btn sm" onclick={resetCatalog}>Default</button>{/if}
          </div>
        </div>
        {#if catInfo}
          <div class="row sub"><span class="path" title={catInfo.path}>{catInfo.path}</span></div>
        {/if}
        <div class="row hintrow">Press <kbd>L</kbd> for dim / lights-out · <kbd>G</kbd> grid · <kbd>D</kbd> details.</div>
      </div>
    {/if}

    <!-- body: viewport (+ optional right filmstrip) -->
    <div class="body">
      <div class="viewport" class:lit={dimLevel > 0}>
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
          <Loupe item={active} />
        {:else if viewMode === "details"}
          <DetailsView
            items={view}
            {activeIndex}
            {selected}
            onrowclick={gridCellClick}
            onrowdblclick={(i) => { setActiveTo(i); setView("loupe"); }}
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
</div>

<style>
  .app { display: flex; height: 100vh; overflow: hidden; }
  .tree { display: flex; flex-direction: column; background: var(--bg-panel); border-right: 1px solid var(--border); flex: 0 0 auto; min-width: 0; }
  .tree-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 9px 10px; border-bottom: 1px solid var(--border); }
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

  .tagwrap { position: relative; }
  .tagmenu { position: absolute; top: 32px; left: 0; z-index: 30; min-width: 180px; max-height: 320px; overflow-y: auto; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 9px; box-shadow: var(--shadow); padding: 5px; }
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
  .pop .row.sub { padding-left: 6px; }
  .pop .path { color: var(--text-dim); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pop .hintrow { color: var(--text-faint); font-size: 12px; }
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
