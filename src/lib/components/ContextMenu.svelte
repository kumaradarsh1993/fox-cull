<script lang="ts" module>
  // A single menu entry: either a divider or an actionable row.
  export type MenuEntry =
    | { separator: true }
    | {
        label: string;
        icon?: string;
        danger?: boolean;
        disabled?: boolean;
        on?: boolean; // shows a checkmark / active accent
        action: () => void;
      };
</script>

<script lang="ts">
  // A lightweight floating context menu positioned at the cursor. Replaces the
  // webview's native menu (which leaked browser items like "Save image"). Closes
  // on outside click, right-click, Escape, scroll, resize, or window blur.
  let {
    x,
    y,
    entries,
    onclose,
  }: { x: number; y: number; entries: MenuEntry[]; onclose: () => void } = $props();

  let el = $state<HTMLDivElement | null>(null);
  // Clamped position; null until the menu is measured (then it can't spill off
  // screen). Pre-measure we render at the raw cursor point (x/y from props).
  let pos = $state<{ x: number; y: number } | null>(null);

  $effect(() => {
    const e = el;
    if (!e) return;
    const r = e.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    pos = {
      x: Math.max(6, Math.min(x, vw - r.width - 6)),
      y: Math.max(6, Math.min(y, vh - r.height - 6)),
    };
  });

  function choose(entry: Extract<MenuEntry, { label: string }>) {
    if (entry.disabled) return;
    entry.action();
    onclose();
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }}
  onresize={onclose}
  onblur={onclose}
/>

<!-- transparent backdrop: any click (incl. right-click) closes the menu -->
<button
  class="cm-backdrop"
  aria-label="Close menu"
  onclick={onclose}
  oncontextmenu={(e) => {
    e.preventDefault();
    onclose();
  }}
></button>

<div class="cm" bind:this={el} style="left:{pos?.x ?? x}px; top:{pos?.y ?? y}px" role="menu">
  {#each entries as entry, idx (idx)}
    {#if "separator" in entry}
      <div class="cm-sep"></div>
    {:else}
      <button
        class="cm-item"
        class:danger={entry.danger}
        class:on={entry.on}
        disabled={entry.disabled}
        role="menuitem"
        onclick={() => choose(entry)}
      >
        <span class="cm-ic">{entry.icon ?? ""}</span>
        <span class="cm-lbl">{entry.label}</span>
        {#if entry.on}<span class="cm-check">✓</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .cm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
  }
  .cm {
    position: fixed;
    z-index: 201;
    min-width: 196px;
    padding: 5px;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    font-size: 12.5px;
    user-select: none;
  }
  .cm-item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    text-align: left;
    padding: 6px 9px;
    border-radius: 6px;
    color: var(--text);
    line-height: 1.2;
  }
  .cm-item:hover {
    background: var(--bg-hover);
  }
  .cm-item:disabled {
    opacity: 0.4;
    pointer-events: none;
  }
  .cm-item.danger:hover {
    background: color-mix(in srgb, var(--reject) 22%, var(--bg-hover));
  }
  .cm-ic {
    width: 16px;
    text-align: center;
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .cm-item.danger .cm-ic {
    color: var(--reject);
  }
  .cm-item.on .cm-ic {
    color: var(--accent);
  }
  .cm-lbl {
    flex: 1;
  }
  .cm-check {
    color: var(--accent);
    font-size: 11px;
  }
  .cm-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
