<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { resetThumbs } from "$lib/thumbnail-loader";
  import { getLastRoot, setLastRoot } from "$lib/persist";
  import {
    LABELS,
    LABEL_BY_DIGIT,
    LABEL_VAR,
    type MediaItem,
    type TreeDir,
    type Filter,
    type TrashOutcome,
  } from "$lib/types";
  import TreeNode from "$lib/components/TreeNode.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import Loupe from "$lib/components/Loupe.svelte";
  import VirtualGrid from "$lib/components/VirtualGrid.svelte";
  import VirtualStrip from "$lib/components/VirtualStrip.svelte";

  let root = $state<string | null>(null);
  let rootNode = $state<TreeDir | null>(null);
  let currentDir = $state<string | null>(null);
  let items = $state<MediaItem[]>([]);
  let loading = $state(false);
  let writable = $state(true);
  let includeSub = $state(true);

  let mode = $state<"grid" | "loupe">("grid");
  let activeIndex = $state(0);
  let selected = $state<Set<string>>(new Set());

  let filter = $state<Filter>({ minRating: 0, label: null, flag: null });

  let gridComp = $state<{ scrollToIndex: (i: number) => void } | null>(null);

  let sweep = $state<{ open: boolean; paths: string[]; result: TrashOutcome | null }>({
    open: false,
    paths: [],
    result: null,
  });
  let toast = $state<string | null>(null);

  const basename = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  let filtered = $derived(
    items.filter((it) => {
      if (filter.minRating > 0 && it.rating < filter.minRating) return false;
      if (filter.label && it.label !== filter.label) return false;
      if (filter.flag === "reject" && it.flag !== "reject") return false;
      if (filter.flag === "pick" && it.flag !== "pick") return false;
      if (filter.flag === "unflagged" && it.flag) return false;
      return true;
    }),
  );

  let active = $derived(
    filtered.length ? filtered[Math.min(activeIndex, filtered.length - 1)] : null,
  );
  let rejectedCount = $derived(items.filter((i) => i.flag === "reject").length);
  let pickCount = $derived(items.filter((i) => i.flag === "pick").length);

  $effect(() => {
    if (activeIndex > filtered.length - 1) activeIndex = Math.max(0, filtered.length - 1);
  });

  onMount(async () => {
    const last = await getLastRoot();
    if (last) await applyRoot(last);
  });

  async function chooseRoot() {
    const picked = await api.pickFolder();
    if (picked) {
      await applyRoot(picked);
      await setLastRoot(picked);
    }
  }

  async function applyRoot(r: string) {
    try {
      await api.setLibraryRoot(r);
    } catch (e) {
      toast = `Couldn't open that folder: ${e}`;
      return;
    }
    root = r;
    rootNode = { name: basename(r), path: r, has_children: true };
    currentDir = null;
    items = [];
  }

  async function openFolder(dir: string) {
    currentDir = dir;
    loading = true;
    resetThumbs();
    selected = new Set();
    const t0 = performance.now();
    try {
      items = await api.listFolderMedia(dir, includeSub);
      writable = await api.folderWritable(dir);
    } catch (e) {
      items = [];
      toast = `Failed to read folder: ${e}`;
    }
    const ms = Math.round(performance.now() - t0);
    api.logEvent(`folder-open ${basename(dir)} recursive=${includeSub} items=${items.length} roundtrip=${ms}ms`);
    activeIndex = 0;
    if (filtered.length) selected = new Set([filtered[0].path]);
    loading = false;
    // Mark when the webview has painted the first frame after data arrived —
    // the gap between roundtrip and this reveals client-side render cost.
    requestAnimationFrame(() =>
      requestAnimationFrame(() =>
        api.logEvent(`grid-painted ${basename(dir)} +${Math.round(performance.now() - t0)}ms total`),
      ),
    );
  }

  async function toggleSub() {
    includeSub = !includeSub;
    if (currentDir) await openFolder(currentDir);
  }

  function targets(): MediaItem[] {
    if (selected.size > 1) return items.filter((i) => selected.has(i.path));
    return active ? [active] : [];
  }

  function scrollActive() {
    gridComp?.scrollToIndex(activeIndex);
  }

  function setActiveTo(i: number) {
    activeIndex = Math.max(0, Math.min(i, filtered.length - 1));
    const a = filtered[activeIndex];
    if (a) selected = new Set([a.path]);
    scrollActive();
  }

  function move(delta: number) {
    setActiveTo(activeIndex + delta);
  }

  // For a single target we TOGGLE; for a multi-selection we SET (intent of
  // pressing X on 500 selected photos is "reject them all", not toggle each).
  function rate(r: number) {
    const ts = targets();
    if (ts.length === 1) {
      const it = ts[0];
      it.rating = it.rating === r ? 0 : r;
      api.setRating(it.path, it.rating).catch(() => {});
    } else if (ts.length > 1) {
      for (const it of ts) it.rating = r;
      api.setRatingMany(ts.map((i) => i.path), r).catch(() => {});
    }
  }

  function label(key: string) {
    const ts = targets();
    if (ts.length === 1) {
      const it = ts[0];
      it.label = it.label === key ? null : key;
      api.setLabel(it.path, it.label).catch(() => {});
    } else if (ts.length > 1) {
      for (const it of ts) it.label = key;
      api.setLabelMany(ts.map((i) => i.path), key).catch(() => {});
    }
  }

  function flag(f: "pick" | "reject") {
    const ts = targets();
    if (ts.length === 1) {
      const it = ts[0];
      it.flag = it.flag === f ? null : f;
      api.setFlag(it.path, it.flag).catch(() => {});
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

  function selectAllFiltered() {
    selected = new Set(filtered.map((i) => i.path));
  }

  function rejectSelected() {
    const sel = items.filter((i) => selected.has(i.path));
    if (!sel.length) return;
    for (const it of sel) it.flag = "reject";
    api.setFlagMany(sel.map((i) => i.path), "reject").catch(() => {});
  }

  function gridCellClick(e: MouseEvent, i: number) {
    const it = filtered[i];
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

  async function openSweep() {
    const paths = await api.listRejected();
    sweep = { open: true, paths, result: null };
  }

  async function confirmSweep() {
    const result = await api.deleteToTrash(sweep.paths);
    sweep = { ...sweep, result };
    if (currentDir) await openFolder(currentDir);
    toast =
      `Moved ${result.deleted} file(s) to the Recycle Bin/Trash` +
      (result.failed.length ? ` · ${result.failed.length} failed` : "");
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
    if (!root) return;

    if (e.key === "ArrowRight" || e.key === "ArrowDown") { move(1); e.preventDefault(); return; }
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") { move(-1); e.preventDefault(); return; }
    if (e.key === "Enter") { mode = mode === "grid" ? "loupe" : "grid"; e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (mode === "loupe") mode = "grid";
      else selected = active ? new Set([active.path]) : new Set();
      return;
    }
    if (e.key >= "1" && e.key <= "5") { rate(Number(e.key)); return; }
    if (e.key === "`") { rate(0); return; }
    if (e.key in LABEL_BY_DIGIT) { label(LABEL_BY_DIGIT[e.key]); return; }
    const k = e.key.toLowerCase();
    if (k === "x") { flag("reject"); return; }
    if (k === "p") { flag("pick"); return; }
    if (k === "u") { unset(); return; }
  }
</script>

<svelte:window {onkeydown} />

<!-- grid cell -->
{#snippet gridCell(item: MediaItem, i: number)}
  <button
    class="cell"
    class:active={i === activeIndex}
    class:selected={selected.has(item.path)}
    class:reject={item.flag === "reject"}
    onclick={(e) => gridCellClick(e, i)}
    ondblclick={() => { setActiveTo(i); mode = "loupe"; }}
  >
    <Thumb {item} size={320} />
    <span class="ov">
      {#if item.label}<span class="lbl-dot" style="background:var({LABEL_VAR[item.label]})"></span>{/if}
      {#if item.flag === "reject"}<span class="fl x">✕</span>{/if}
      {#if item.flag === "pick"}<span class="fl pick">✓</span>{/if}
      {#if item.rating > 0}<span class="stars">{"★".repeat(item.rating)}</span>{/if}
    </span>
  </button>
{/snippet}

<!-- filmstrip cell -->
{#snippet stripCell(item: MediaItem, i: number)}
  <button
    class="scell"
    class:active={i === activeIndex}
    class:reject={item.flag === "reject"}
    onclick={() => setActiveTo(i)}
    ondblclick={() => { setActiveTo(i); mode = "loupe"; }}
    title={item.name}
  >
    <!-- same size as the grid so the filmstrip reuses the cached decode (no 2nd decode) -->
    <Thumb {item} size={320} />
    {#if item.label}<span class="s-lbl" style="background:var({LABEL_VAR[item.label]})"></span>{/if}
    {#if item.rating > 0}<span class="s-stars">{"★".repeat(item.rating)}</span>{/if}
    {#if item.flag === "reject"}<span class="s-x">✕</span>{/if}
    {#if item.flag === "pick"}<span class="s-pick">✓</span>{/if}
  </button>
{/snippet}

<div class="app">
  <aside class="tree">
    <div class="tree-head">
      <span class="brand">🦊 fox-cull</span>
      <button class="btn" onclick={chooseRoot} title="Choose library folder">Folder…</button>
    </div>
    <div class="tree-body">
      {#if rootNode}
        <TreeNode node={rootNode} {currentDir} onselect={openFolder} />
      {:else}
        <p class="hint">Pick a folder (your SSD or hard-drive root) to start culling.</p>
      {/if}
    </div>
  </aside>

  <main class="center">
    {#if !writable}
      <div class="banner">
        Read-only mount — culling &amp; rating still save, but the delete sweep is
        disabled here. Run the sweep where this drive is writable.
      </div>
    {/if}

    <div class="bar">
      <div class="seg">
        <span class="lbl">Stars ≥</span>
        {#each [0, 1, 2, 3, 4, 5] as n}
          <button class="chip" class:on={filter.minRating === n} onclick={() => (filter.minRating = n)}>{n === 0 ? "All" : n}</button>
        {/each}
      </div>
      <div class="seg">
        <span class="lbl">Flag</span>
        {#each [["all", null], ["pick", "pick"], ["reject", "reject"], ["unflagged", "unflagged"]] as [name, val]}
          <button class="chip" class:on={filter.flag === val} onclick={() => (filter.flag = val as Filter["flag"])}>{name}</button>
        {/each}
      </div>
      <div class="seg">
        <span class="lbl">Label</span>
        <button class="chip" class:on={filter.label === null} onclick={() => (filter.label = null)}>any</button>
        {#each LABELS as l}
          <button class="dot" class:on={filter.label === l.key} style="background:var({l.varName})" title={l.name} aria-label={l.name} onclick={() => (filter.label = filter.label === l.key ? null : l.key)}></button>
        {/each}
      </div>
      <button class="chip" class:on={includeSub} onclick={toggleSub} title="Show photos from all subfolders too">⊞ Subfolders</button>
      <div class="spacer"></div>
      <button class="btn" onclick={selectAllFiltered} disabled={!filtered.length}>Select all ({filtered.length})</button>
      <button class="btn danger" onclick={rejectSelected} disabled={selected.size === 0}>Reject sel ({selected.size})</button>
      <button class="btn danger" onclick={openSweep} disabled={!writable || rejectedCount === 0}>Delete rejected…</button>
    </div>

    <div class="viewport">
      {#if !root}
        <div class="welcome">
          <h1>🦊 fox-cull</h1>
          <p>A fast, lightweight photo &amp; video culler. Browse in place — nothing is imported or modified.</p>
          <button class="btn accent" onclick={chooseRoot}>Choose a folder to begin</button>
        </div>
      {:else if loading}
        <div class="welcome"><p>Scanning {currentDir ? basename(currentDir) : ""}…</p></div>
      {:else if !currentDir}
        <div class="welcome"><p>Select a folder on the left.</p></div>
      {:else if filtered.length === 0}
        <div class="welcome"><p>No media{includeSub ? "" : " directly"} in <b>{basename(currentDir)}</b>{filter.minRating || filter.flag || filter.label ? " matches the filter" : ""}.</p></div>
      {:else if mode === "loupe"}
        <Loupe item={active} />
      {:else}
        <VirtualGrid items={filtered} {activeIndex} cellMin={176} bind:this={gridComp} cell={gridCell} />
      {/if}
    </div>

    {#if active}
      <div class="info">
        <span class="name" title={active.path}>{active.name}</span>
        <span class="meta">{active.kind} · {activeIndex + 1}/{filtered.length}</span>
        <div class="rate">
          {#each [1, 2, 3, 4, 5] as n}
            <button class="star" class:on={active.rating >= n} onclick={() => rate(n)}>★</button>
          {/each}
        </div>
        {#each LABELS as l}
          <button class="dot sm" class:on={active.label === l.key} style="background:var({l.varName})" title={l.name} aria-label={l.name} onclick={() => label(l.key)}></button>
        {/each}
        <button class="btn" class:on={active.flag === "pick"} onclick={() => flag("pick")}>Pick (P)</button>
        <button class="btn danger" class:on={active.flag === "reject"} onclick={() => flag("reject")}>Reject (X)</button>
        <span class="spacer"></span>
        <span class="counts">✓ {pickCount} · ✕ {rejectedCount}</span>
      </div>
    {/if}

    {#if filtered.length}
      <VirtualStrip items={filtered} {activeIndex} cellSize={108} cell={stripCell} />
    {/if}
  </main>
</div>

{#if sweep.open}
  <div class="modal-bg" role="presentation" onclick={() => (sweep = { open: false, paths: [], result: null })}>
    <div class="modal" role="dialog" onclick={(e) => e.stopPropagation()}>
      {#if sweep.result}
        <h2>Sweep complete</h2>
        <p>Moved <b>{sweep.result.deleted}</b> file(s) to the Recycle Bin / Trash.</p>
        {#if sweep.result.failed.length}
          <p class="err">{sweep.result.failed.length} could not be deleted (read-only or in use).</p>
        {/if}
        <div class="modal-actions">
          <button class="btn accent" onclick={() => (sweep = { open: false, paths: [], result: null })}>Done</button>
        </div>
      {:else}
        <h2>Delete rejected files?</h2>
        <p><b>{sweep.paths.length}</b> file(s) flagged <span class="rj">reject</span> across the whole library will be moved to the <b>Recycle Bin / Trash</b> (recoverable).</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (sweep = { open: false, paths: [], result: null })}>Cancel</button>
          <button class="btn danger" disabled={!sweep.paths.length} onclick={confirmSweep}>Move {sweep.paths.length} to Trash</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if toast}
  <button class="toast" onclick={() => (toast = null)}>{toast}</button>
{/if}

<style>
  .app { display: grid; grid-template-columns: var(--tree-w) 1fr; height: 100vh; overflow: hidden; }
  .tree { display: flex; flex-direction: column; background: var(--bg-panel); border-right: 1px solid var(--border); min-width: 0; }
  .tree-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 10px; border-bottom: 1px solid var(--border); }
  .brand { font-weight: 700; }
  .tree-body { overflow-y: auto; padding: 6px; flex: 1; }
  .hint, .counts { color: var(--text-faint); font-size: 12.5px; }
  .hint { padding: 10px; line-height: 1.5; }

  .center { display: flex; flex-direction: column; min-width: 0; height: 100vh; }

  .bar { display: flex; align-items: center; gap: 14px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bg-panel); flex-wrap: wrap; }
  .seg { display: flex; align-items: center; gap: 4px; }
  .seg .lbl { color: var(--text-faint); font-size: 12px; margin-right: 2px; }
  .spacer { flex: 1; }
  .chip { padding: 3px 8px; border-radius: 5px; font-size: 12.5px; color: var(--text-dim); border: 1px solid transparent; }
  .chip:hover { background: var(--bg-hover); }
  .chip.on { background: var(--accent-dim); color: #fff; }
  .dot { width: 14px; height: 14px; border-radius: 3px; border: 1px solid rgba(0,0,0,0.4); opacity: 0.55; }
  .dot.sm { width: 13px; height: 13px; }
  .dot.on { opacity: 1; outline: 2px solid var(--text); outline-offset: 1px; }

  .viewport { flex: 1; overflow: hidden; background: var(--bg); min-height: 0; }

  .welcome { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; color: var(--text-dim); text-align: center; padding: 24px; }
  .welcome h1 { font-size: 30px; margin: 0; }

  .cell { position: relative; width: 100%; height: 100%; border: 2px solid transparent; border-radius: 5px; overflow: hidden; padding: 0; background: #0d0b08; }
  .cell.selected { border-color: var(--text-dim); }
  .cell.active { border-color: var(--accent); }
  .cell.reject :global(img) { opacity: 0.4; }
  .ov { position: absolute; inset: 0; pointer-events: none; }
  .lbl-dot { position: absolute; top: 5px; right: 5px; width: 12px; height: 12px; border-radius: 3px; border: 1px solid rgba(0,0,0,0.5); }
  .fl { position: absolute; top: 4px; left: 6px; font-weight: 700; text-shadow: 0 1px 3px #000; }
  .fl.x { color: var(--reject); }
  .fl.pick { color: var(--pick); }
  .stars { position: absolute; bottom: 4px; left: 6px; color: var(--star); font-size: 13px; text-shadow: 0 1px 3px #000; }

  .scell { position: relative; width: 100%; height: 100%; border: 2px solid transparent; border-radius: 4px; overflow: hidden; padding: 0; background: #0d0b08; }
  .scell.active { border-color: var(--accent); }
  .scell.reject { opacity: 0.5; }
  .s-lbl { position: absolute; top: 3px; right: 3px; width: 10px; height: 10px; border-radius: 2px; border: 1px solid rgba(0,0,0,0.4); }
  .s-stars { position: absolute; bottom: 2px; left: 3px; font-size: 10px; color: var(--star); text-shadow: 0 1px 2px #000; }
  .s-x { position: absolute; top: 2px; left: 4px; color: var(--reject); font-weight: 700; text-shadow: 0 1px 2px #000; }
  .s-pick { position: absolute; top: 2px; left: 4px; color: var(--pick); font-weight: 700; text-shadow: 0 1px 2px #000; }

  .info { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-top: 1px solid var(--border); background: var(--bg-panel); }
  .info .name { font-weight: 600; max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .info .meta { color: var(--text-faint); font-size: 12px; }
  .rate { display: flex; }
  .star { color: var(--text-faint); font-size: 16px; }
  .star.on { color: var(--star); }
  .btn.on { border-color: var(--accent); }

  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 50; border: none; }
  .modal { background: var(--bg-elev); border: 1px solid var(--border); border-radius: 10px; padding: 22px; width: min(460px, 90vw); }
  .modal h2 { margin: 0 0 10px; }
  .modal p { color: var(--text-dim); line-height: 1.5; }
  .modal .err { color: var(--reject); }
  .rj { color: var(--reject); font-weight: 600; }
  .modal-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 18px; }

  .toast { position: fixed; bottom: 18px; left: 50%; transform: translateX(-50%); background: var(--bg-elev); border: 1px solid var(--accent-dim); color: var(--text); padding: 10px 16px; border-radius: 8px; z-index: 60; box-shadow: 0 6px 24px rgba(0,0,0,0.5); }
</style>
