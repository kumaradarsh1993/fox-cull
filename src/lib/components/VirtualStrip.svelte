<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  let {
    items,
    activeIndex = 0,
    cellSize = 108,
    gap = 6,
    overscan = 6,
    cell,
  }: {
    items: T[];
    activeIndex?: number;
    cellSize?: number;
    gap?: number;
    overscan?: number;
    cell: Snippet<[T, number]>;
  } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollLeft = $state(0);
  let vpWidth = $state(0);

  let step = $derived(cellSize + gap);
  let totalW = $derived(Math.max(0, items.length * step - gap));
  let first = $derived(Math.max(0, Math.floor(scrollLeft / step) - overscan));
  let last = $derived(
    Math.min(items.length - 1, Math.ceil((scrollLeft + vpWidth) / step) + overscan),
  );

  let visible = $derived.by(() => {
    const out: { item: T; index: number; x: number }[] = [];
    for (let i = first; i <= last; i++) {
      if (i < 0 || i >= items.length) continue;
      out.push({ item: items[i], index: i, x: i * step });
    }
    return out;
  });

  $effect(() => {
    const el = viewport;
    if (!el) return;
    const measure = () => (vpWidth = el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Keep the active cell centered as the user arrows through.
  $effect(() => {
    const el = viewport;
    const i = activeIndex;
    if (!el || !vpWidth) return;
    const x = i * step;
    const target = x - vpWidth / 2 + cellSize / 2;
    const max = Math.max(0, totalW - vpWidth);
    el.scrollLeft = Math.max(0, Math.min(target, max));
  });
</script>

<div
  class="strip"
  bind:this={viewport}
  onscroll={() => {
    if (viewport) scrollLeft = viewport.scrollLeft;
  }}
>
  <div class="canvas" style="width:{totalW}px">
    {#each visible as v (v.index)}
      <div class="cellpos" style="transform:translateX({v.x}px); width:{cellSize}px; height:{cellSize}px">
        {@render cell(v.item, v.index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .strip {
    height: var(--filmstrip-h);
    overflow-x: auto;
    overflow-y: hidden;
    background: var(--bg-panel);
    border-top: 1px solid var(--border);
    padding: 8px 0;
  }
  .canvas {
    position: relative;
    height: calc(var(--filmstrip-h) - 16px);
  }
  .cellpos {
    position: absolute;
    top: 0;
    left: 0;
    will-change: transform;
  }
</style>
