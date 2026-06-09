<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  let {
    items,
    cellMin = 176,
    gap = 6,
    overscanRows = 3,
    activeIndex = 0,
    cell,
  }: {
    items: T[];
    cellMin?: number;
    gap?: number;
    overscanRows?: number;
    activeIndex?: number;
    cell: Snippet<[T, number]>;
  } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let vpWidth = $state(0);
  let vpHeight = $state(0);

  let cols = $derived(Math.max(1, Math.floor((vpWidth + gap) / (cellMin + gap))));
  let cellW = $derived(cols > 0 ? (vpWidth - gap * (cols - 1)) / cols : cellMin);
  let rowH = $derived(cellW + gap);
  let rowCount = $derived(Math.ceil(items.length / cols));
  let totalH = $derived(Math.max(0, rowCount * rowH - gap));

  let firstRow = $derived(Math.max(0, Math.floor(scrollTop / rowH) - overscanRows));
  let lastRow = $derived(
    Math.min(rowCount - 1, Math.ceil((scrollTop + vpHeight) / rowH) + overscanRows),
  );

  let visible = $derived.by(() => {
    const out: { item: T; index: number; x: number; y: number }[] = [];
    if (!items.length) return out;
    for (let r = firstRow; r <= lastRow; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        if (i >= items.length) break;
        out.push({ item: items[i], index: i, x: c * (cellW + gap), y: r * rowH });
      }
    }
    return out;
  });

  // Measure the viewport (and react to window/pane resizes).
  $effect(() => {
    const el = viewport;
    if (!el) return;
    const measure = () => {
      vpWidth = el.clientWidth;
      vpHeight = el.clientHeight;
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Coalesce scroll events to one read per animation frame. The native scroll
  // event can fire many times per frame during a fast flick; recomputing the
  // visible set (and mounting/unmounting cells + their image requests) on each
  // one is wasted main-thread work that shows up as scroll jank. One rAF-aligned
  // update per frame keeps the grid smooth.
  let scrollRAF = 0;
  function onScroll() {
    if (scrollRAF) return;
    scrollRAF = requestAnimationFrame(() => {
      scrollRAF = 0;
      if (viewport) scrollTop = viewport.scrollTop;
    });
  }

  /** Keep a given index visible — used by keyboard navigation. With `center`,
   *  place it mid-viewport (used to restore position when returning from Focus). */
  export function scrollToIndex(i: number, center = false) {
    const el = viewport;
    if (!el || cols <= 0) return;
    const row = Math.floor(i / cols);
    const y = row * rowH;
    if (center) {
      el.scrollTop = Math.max(0, y - (vpHeight - cellW) / 2);
      return;
    }
    if (y < el.scrollTop) el.scrollTop = y;
    else if (y + cellW > el.scrollTop + vpHeight) el.scrollTop = y + cellW - vpHeight;
  }
</script>

<div class="vp" bind:this={viewport} onscroll={onScroll}>
  <div class="canvas" style="height:{totalH}px">
    {#each visible as v (v.index)}
      <div
        class="cellpos"
        class:active={v.index === activeIndex}
        style="transform:translate({v.x}px,{v.y}px); width:{cellW}px; height:{cellW}px"
      >
        {@render cell(v.item, v.index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .vp {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .canvas {
    position: relative;
    width: 100%;
  }
  .cellpos {
    position: absolute;
    top: 0;
    left: 0;
    will-change: transform;
  }
</style>
