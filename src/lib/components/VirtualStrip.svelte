<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  let {
    items,
    activeIndex = 0,
    cellSize = 108,
    gap = 6,
    overscan = 6,
    orientation = "h",
    cell,
  }: {
    items: T[];
    activeIndex?: number;
    cellSize?: number;
    gap?: number;
    overscan?: number;
    orientation?: "h" | "v";
    cell: Snippet<[T, number]>;
  } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollPos = $state(0);
  let vpMain = $state(0); // viewport length along the scroll axis

  let step = $derived(cellSize + gap);
  let total = $derived(Math.max(0, items.length * step - gap));
  let first = $derived(Math.max(0, Math.floor(scrollPos / step) - overscan));
  let last = $derived(Math.min(items.length - 1, Math.ceil((scrollPos + vpMain) / step) + overscan));

  let visible = $derived.by(() => {
    const out: { item: T; index: number; pos: number }[] = [];
    for (let i = first; i <= last; i++) {
      if (i < 0 || i >= items.length) continue;
      out.push({ item: items[i], index: i, pos: i * step });
    }
    return out;
  });

  function onscroll() {
    if (!viewport) return;
    scrollPos = orientation === "h" ? viewport.scrollLeft : viewport.scrollTop;
  }

  // Measure viewport length along the scroll axis; also wire a non-passive wheel
  // handler so a normal vertical mouse wheel scrolls a HORIZONTAL strip.
  $effect(() => {
    const el = viewport;
    if (!el) return;
    const measure = () => (vpMain = orientation === "h" ? el.clientWidth : el.clientHeight);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);

    let onWheel: ((e: WheelEvent) => void) | null = null;
    if (orientation === "h") {
      onWheel = (e: WheelEvent) => {
        const d = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
        if (d !== 0) {
          el.scrollLeft += d;
          e.preventDefault();
        }
      };
      el.addEventListener("wheel", onWheel, { passive: false });
    }
    return () => {
      ro.disconnect();
      if (onWheel) el.removeEventListener("wheel", onWheel);
    };
  });

  // Keep the active cell centered as the user arrows through.
  $effect(() => {
    const el = viewport;
    const i = activeIndex;
    if (!el || !vpMain) return;
    const target = i * step - vpMain / 2 + cellSize / 2;
    const v = Math.max(0, Math.min(target, Math.max(0, total - vpMain)));
    if (orientation === "h") el.scrollLeft = v;
    else el.scrollTop = v;
  });
</script>

<div class="strip {orientation}" bind:this={viewport} {onscroll} style="--cell:{cellSize}px">
  <div class="canvas" style={orientation === "h" ? `width:${total}px` : `height:${total}px`}>
    {#each visible as v (v.index)}
      <div
        class="cellpos"
        style="transform:{orientation === 'h' ? `translateX(${v.pos}px)` : `translateY(${v.pos}px)`}; width:var(--cell); height:var(--cell)"
      >
        {@render cell(v.item, v.index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .strip {
    background: var(--bg-panel);
  }
  .strip.h {
    width: 100%;
    height: 100%;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .strip.v {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .canvas {
    position: relative;
    height: 100%;
  }
  .strip.v .canvas {
    width: 100%;
  }
  .cellpos {
    position: absolute;
  }
  .strip.h .cellpos {
    top: calc((100% - var(--cell)) / 2);
    left: 0;
  }
  .strip.v .cellpos {
    left: calc((100% - var(--cell)) / 2);
    top: 0;
  }
</style>
