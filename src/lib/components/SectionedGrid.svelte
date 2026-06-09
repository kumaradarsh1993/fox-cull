<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  // A virtualized grid split into labeled sections (month headers), sharing the
  // same flat `items` array + global index space as VirtualGrid so keyboard
  // navigation and active-cell highlighting stay identical. `groups` gives the
  // section labels and their item counts, IN the same order as `items`.
  let {
    items,
    groups,
    cellMin = 176,
    gap = 6,
    headerH = 38,
    overscanRows = 3,
    activeIndex = 0,
    cell,
  }: {
    items: T[];
    groups: { label: string; count: number }[];
    cellMin?: number;
    gap?: number;
    headerH?: number;
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

  type Row =
    | { type: "header"; key: string; label: string; y: number; h: number }
    | { type: "cells"; key: string; idxs: number[]; y: number; h: number };

  // Flatten the sections into a list of positioned rows (one header row + N cell
  // rows per group). Recomputed only when layout inputs change, not on scroll.
  let rows = $derived.by(() => {
    const out: Row[] = [];
    let y = 0;
    let gi = 0; // running global item index
    for (let g = 0; g < groups.length; g++) {
      const grp = groups[g];
      out.push({ type: "header", key: `h${g}`, label: grp.label, y, h: headerH });
      y += headerH;
      let remaining = grp.count;
      while (remaining > 0) {
        const n = Math.min(cols, remaining);
        const idxs: number[] = [];
        for (let k = 0; k < n; k++) idxs.push(gi + k);
        out.push({ type: "cells", key: `c${gi}`, idxs, y, h: rowH });
        y += rowH;
        gi += n;
        remaining -= n;
      }
    }
    return out;
  });

  let totalH = $derived(rows.length ? rows[rows.length - 1].y + rows[rows.length - 1].h : 0);

  let visible = $derived.by(() => {
    const pad = overscanRows * rowH;
    const lo = scrollTop - pad;
    const hi = scrollTop + vpHeight + pad;
    const out: Row[] = [];
    for (const r of rows) {
      if (r.y + r.h < lo) continue;
      if (r.y > hi) break;
      out.push(r);
    }
    return out;
  });

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

  // Coalesce scroll events to one visible-set recompute per animation frame (see
  // VirtualGrid) so fast flicks don't thrash the main thread mounting cells.
  let scrollRAF = 0;
  function onScroll() {
    if (scrollRAF) return;
    scrollRAF = requestAnimationFrame(() => {
      scrollRAF = 0;
      if (viewport) scrollTop = viewport.scrollTop;
    });
  }

  /** Bring a global item index into view (used by keyboard navigation). With
   *  `center`, place it mid-viewport (restores position on return from Focus). */
  export function scrollToIndex(i: number, center = false) {
    const el = viewport;
    if (!el) return;
    const r = rows.find(
      (rw) => rw.type === "cells" && rw.idxs.length > 0 && i >= rw.idxs[0] && i <= rw.idxs[rw.idxs.length - 1],
    );
    if (!r) return;
    if (center) {
      el.scrollTop = Math.max(0, r.y - (vpHeight - rowH) / 2);
      return;
    }
    // Reveal the month header too when scrolling up to a section's first row.
    if (r.y - headerH < el.scrollTop) el.scrollTop = Math.max(0, r.y - headerH);
    else if (r.y + rowH > el.scrollTop + vpHeight) el.scrollTop = r.y + rowH - vpHeight;
  }
</script>

<div class="vp" bind:this={viewport} onscroll={onScroll}>
  <div class="canvas" style="height:{totalH}px">
    {#each visible as r (r.key)}
      {#if r.type === "header"}
        <div class="hdr" style="transform:translateY({r.y}px); height:{r.h}px">
          {r.label}
        </div>
      {:else}
        {#each r.idxs as gi, c (gi)}
          <div
            class="cellpos"
            class:active={gi === activeIndex}
            style="transform:translate({c * (cellW + gap)}px, {r.y}px); width:{cellW}px; height:{cellW}px"
          >
            {@render cell(items[gi], gi)}
          </div>
        {/each}
      {/if}
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
  .hdr {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    padding: 0 2px 4px;
  }
  .cellpos {
    position: absolute;
    top: 0;
    left: 0;
    will-change: transform;
  }
</style>
