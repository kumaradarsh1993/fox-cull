<script lang="ts">
  import { api } from "$lib/api";
  import type { TreeDir } from "$lib/types";
  import Self from "./TreeNode.svelte";

  let {
    node,
    currentDir,
    onselect,
    depth = 0,
    count = null,
    countsGen = 0,
  }: {
    node: TreeDir;
    currentDir: string | null;
    onselect: (path: string) => void;
    depth?: number;
    /** Recursive media count for THIS folder (given by the parent), or null. */
    count?: number | null;
    /** Bumped by the tree's ↻ button to force open nodes to recount. */
    countsGen?: number;
  } = $props();

  let open = $state(false);
  let kids = $state<TreeDir[] | null>(null);
  let kidCounts = $state<Record<string, number>>({});
  let loading = $state(false);

  // Optimistic chevron: every folder claims children (list_tree no longer probes,
  // to stay fast); once an expand turns up no subfolders we hide it.
  let showChevron = $derived(node.has_children && !(kids !== null && kids.length === 0));

  async function loadKids() {
    loading = true;
    try {
      kids = await api.listTree(node.path);
    } catch {
      kids = [];
    }
    loading = false;
    fetchCounts(); // fill child badges (cached → instant; else background)
  }

  async function fetchCounts(recompute = false) {
    if (!kids || !kids.length) return;
    try {
      const cs = await api.folderCounts(
        kids.map((k) => k.path),
        recompute,
      );
      const m: Record<string, number> = { ...kidCounts };
      for (const c of cs) m[c.path] = c.count;
      kidCounts = m;
    } catch {
      /* counts are best-effort — leave badges blank on failure */
    }
  }

  async function toggle() {
    open = !open;
    if (open && kids === null) await loadKids();
  }

  // Recount when the user hits ↻ (countsGen changes) and we're expanded. The
  // sentinel start avoids a spurious recount on mount (and capturing the prop).
  let lastGen = -1;
  $effect(() => {
    if (countsGen !== lastGen) {
      lastGen = countsGen;
      if (open && kids && kids.length) fetchCounts(true);
    }
  });

  /** Is `dir` strictly inside `ancestor` (path-boundary-aware, case-insensitive
   *  for Windows drive letters/folders)? */
  function isUnder(dir: string, ancestor: string): boolean {
    const a = ancestor.toLowerCase().replace(/[\\/]+$/, "");
    const d = dir.toLowerCase();
    return (
      d.length > a.length &&
      d.startsWith(a) &&
      (d[a.length] === "\\" || d[a.length] === "/")
    );
  }

  // Cascade-open to the folder that's actually open: when the current folder
  // lives under this node, auto-expand it (each child then does the same, so the
  // chain unfolds down to the selected folder — e.g. restoring the last session).
  // Done at most once per currentDir value, so manually collapsing an ancestor
  // afterwards sticks instead of fighting the effect.
  let autoExpandedFor: string | null = null;
  $effect(() => {
    const cd = currentDir;
    if (!cd || cd === autoExpandedFor) return;
    if (isUnder(cd, node.path)) {
      autoExpandedFor = cd;
      if (!open) {
        open = true;
        if (kids === null) loadKids();
      }
    }
  });

  // Keep the selected folder's row visible in the (scrollable) tree pane.
  let rowEl = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (currentDir === node.path) rowEl?.scrollIntoView({ block: "nearest" });
  });
</script>

<div
  class="trow"
  class:active={currentDir === node.path}
  style="padding-left:{4 + depth * 14}px"
  bind:this={rowEl}
>
  {#if showChevron}
    <button
      class="chev"
      class:open
      onclick={toggle}
      aria-label={open ? "Collapse" : "Expand"}
      title={open ? "Collapse" : "Expand"}
    >
      {open ? "▾" : "▸"}
    </button>
  {:else}
    <span class="chev-spacer"></span>
  {/if}
  <button class="tname" title={node.path} onclick={() => onselect(node.path)}>
    <span class="ico">{open ? "📂" : "📁"}</span>
    <span class="label">{node.name}</span>
    {#if count != null}<span class="cnt">{count.toLocaleString()}</span>{/if}
  </button>
</div>

{#if open && kids}
  {#each kids as k (k.path)}
    <Self
      node={k}
      {currentDir}
      {onselect}
      depth={depth + 1}
      count={kidCounts[k.path] ?? null}
      {countsGen}
    />
  {/each}
{/if}

<style>
  .trow {
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    border-radius: 6px;
  }
  .trow.active {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .trow.active .label {
    color: var(--accent);
    font-weight: 600;
  }

  /* Bigger, obvious expand/collapse target — clearly separate from the row's
     select action, with a filled triangle that flips on state. */
  .chev {
    flex: 0 0 auto;
    width: 24px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--text-faint);
    border-radius: 5px;
  }
  .chev:hover {
    background: var(--bg-hover);
    color: var(--accent);
  }
  .chev-spacer {
    flex: 0 0 auto;
    width: 24px;
  }

  .tname {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 6px;
    text-align: left;
    border-radius: 5px;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.2;
  }
  .tname:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .ico {
    font-size: 13px;
    flex: 0 0 auto;
    opacity: 0.85;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Lightroom-style file count, right-aligned and muted. */
  .cnt {
    flex: 0 0 auto;
    margin-left: auto;
    padding-left: 6px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--text-faint);
  }
  .trow.active .cnt {
    color: color-mix(in srgb, var(--accent) 70%, var(--text-faint));
  }
</style>
