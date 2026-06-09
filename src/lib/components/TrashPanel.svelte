<script lang="ts">
  import Thumb from "./Thumb.svelte";
  import type { MediaItem, TrashItem } from "$lib/types";

  let {
    items,
    onclose,
    onrestore,
    onpurge,
  }: {
    items: TrashItem[];
    onclose: () => void;
    onrestore: (stored: string[]) => Promise<void>;
    onpurge: (stored: string[]) => Promise<void>;
  } = $props();

  let sel = $state<Set<string>>(new Set());
  let busy = $state(false);

  // Thumb wants a MediaItem — synthesize one pointing at the file in the recycle
  // folder (its real path), so its cached thumbnail/poster renders as usual.
  function asMedia(it: TrashItem): MediaItem {
    return {
      kind: it.kind,
      path: it.path,
      name: it.name,
      ext: it.ext,
      rel: "",
      mtime: it.deleted_at,
      size: 0,
      rating: 0,
      label: null,
      flag: null,
      tags: [],
    };
  }

  function toggle(stored: string) {
    const next = new Set(sel);
    if (next.has(stored)) next.delete(stored);
    else next.add(stored);
    sel = next;
  }
  function selectAll() {
    sel = sel.size === items.length ? new Set() : new Set(items.map((i) => i.stored));
  }

  const when = (s: number) =>
    new Date(s * 1000).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });

  function picked(): string[] {
    return sel.size ? [...sel] : [];
  }
  async function restore(stored: string[]) {
    if (!stored.length || busy) return;
    busy = true;
    try {
      await onrestore(stored);
      sel = new Set();
    } finally {
      busy = false;
    }
  }
  async function purge(stored: string[]) {
    if (!stored.length || busy) return;
    busy = true;
    try {
      await onpurge(stored);
      sel = new Set();
    } finally {
      busy = false;
    }
  }
  function emptyAll() {
    if (!items.length) return;
    if (confirm(`Permanently delete all ${items.length} item(s) in the Trash? This can't be undone.`))
      purge(items.map((i) => i.stored));
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" onclick={onclose} role="presentation"></div>
<div class="panel" role="dialog" aria-label="Trash">
  <header>
    <h2>🗑 Trash</h2>
    <span class="count">{items.length} item{items.length === 1 ? "" : "s"}</span>
    <span class="grow"></span>
    {#if items.length}
      <button class="b" onclick={selectAll}>
        {sel.size === items.length ? "Deselect all" : "Select all"}
      </button>
      <button class="b ok" disabled={busy || sel.size === 0} onclick={() => restore(picked())}>
        ↩ Restore{sel.size ? ` ${sel.size}` : ""}
      </button>
      <button class="b danger" disabled={busy || sel.size === 0} onclick={() => purge(picked())}>
        ✕ Delete forever{sel.size ? ` ${sel.size}` : ""}
      </button>
      <button class="b danger ghost" disabled={busy} onclick={emptyAll}>Empty trash</button>
    {/if}
    <button class="x" onclick={onclose} title="Close (Esc)">✕</button>
  </header>

  {#if !items.length}
    <div class="empty">
      <p>Trash is empty.</p>
      <p class="sub">Rejected photos you delete (with "In-app Trash" selected) land here, so you can preview and restore them.</p>
    </div>
  {:else}
    <div class="grid">
      {#each items as it (it.stored)}
        <div
          class="cell"
          class:on={sel.has(it.stored)}
          onclick={() => toggle(it.stored)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && toggle(it.stored)}
        >
          <div class="thumbwrap"><Thumb item={asMedia(it)} size={320} /></div>
          <div class="meta">
            <span class="nm" title={it.orig}>{it.name}</span>
            <span class="dt">{when(it.deleted_at)}</span>
          </div>
          {#if sel.has(it.stored)}<span class="tick">✓</span>{/if}
          <div class="rowacts">
            <button class="mini ok" title="Restore to original location" onclick={(e) => { e.stopPropagation(); restore([it.stored]); }}>↩</button>
            <button class="mini danger" title="Delete forever" onclick={(e) => { e.stopPropagation(); purge([it.stored]); }}>✕</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 100;
  }
  .panel {
    position: fixed;
    inset: 5% 6%;
    z-index: 101;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }
  header h2 {
    margin: 0;
    font-size: 15px;
  }
  .count {
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .grow {
    flex: 1;
  }
  .b {
    padding: 5px 11px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    font-size: 12.5px;
  }
  .b:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .b:disabled {
    opacity: 0.45;
  }
  .b.ok {
    border-color: var(--accent);
    color: var(--accent);
  }
  .b.danger {
    border-color: var(--reject);
    color: var(--reject);
  }
  .b.danger.ghost {
    opacity: 0.8;
  }
  .x {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .x:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--text-dim);
  }
  .empty .sub {
    font-size: 12.5px;
    color: var(--text-faint);
    max-width: 420px;
    text-align: center;
  }
  .grid {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px;
    align-content: start;
  }
  .cell {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--viewport-bg);
    cursor: pointer;
  }
  .cell.on {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .thumbwrap {
    aspect-ratio: 1;
    width: 100%;
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px 7px 7px;
  }
  .nm {
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dt {
    font-size: 10.5px;
    color: var(--text-faint);
  }
  .tick {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-on);
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .rowacts {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .cell:hover .rowacts {
    opacity: 1;
  }
  .mini {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .mini.ok:hover {
    background: var(--accent);
    color: var(--accent-on);
  }
  .mini.danger:hover {
    background: var(--reject);
  }
</style>
