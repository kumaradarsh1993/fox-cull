<script lang="ts">
  import Thumb from "./Thumb.svelte";
  import { LABELS, LABEL_VAR, type MediaItem } from "$lib/types";

  let {
    items,
    activeIndex = 0,
    selected,
    onrowclick,
    onrowdblclick,
  }: {
    items: MediaItem[];
    activeIndex?: number;
    selected: Set<string>;
    onrowclick: (e: MouseEvent, i: number) => void;
    onrowdblclick: (i: number) => void;
  } = $props();

  const ROW = 46;
  const OVERSCAN = 8;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let vpHeight = $state(0);

  let total = $derived(items.length * ROW);
  let first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  let last = $derived(
    Math.min(items.length - 1, Math.ceil((scrollTop + vpHeight) / ROW) + OVERSCAN),
  );

  let visible = $derived.by(() => {
    const out: { item: MediaItem; index: number; y: number }[] = [];
    for (let i = first; i <= last; i++) {
      if (i < 0 || i >= items.length) continue;
      out.push({ item: items[i], index: i, y: i * ROW });
    }
    return out;
  });

  $effect(() => {
    const el = viewport;
    if (!el) return;
    const measure = () => (vpHeight = el.clientHeight);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Keep the active row visible as the user arrows through.
  $effect(() => {
    const el = viewport;
    const i = activeIndex;
    if (!el || !vpHeight) return;
    const y = i * ROW;
    if (y < el.scrollTop) el.scrollTop = y;
    else if (y + ROW > el.scrollTop + vpHeight) el.scrollTop = y + ROW - vpHeight;
  });

  const KB = 1024;
  function fmtSize(n: number): string {
    if (!n) return "—";
    if (n < KB) return `${n} B`;
    if (n < KB * KB) return `${(n / KB).toFixed(0)} KB`;
    if (n < KB * KB * KB) return `${(n / (KB * KB)).toFixed(1)} MB`;
    return `${(n / (KB * KB * KB)).toFixed(2)} GB`;
  }
  function fmtDate(epochSecs: number): string {
    if (!epochSecs) return "—";
    return new Date(epochSecs * 1000).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
  const labelColor = (k: string) => LABEL_VAR[k] ?? "--text-faint";
</script>

<div class="head">
  <span class="c-thumb"></span>
  <span class="c-name">Name</span>
  <span class="c-marks">Marks</span>
  <span class="c-type">Type</span>
  <span class="c-size">Size</span>
  <span class="c-date">Date</span>
</div>
<div
  class="vp"
  bind:this={viewport}
  onscroll={() => {
    if (viewport) scrollTop = viewport.scrollTop;
  }}
>
  <div class="canvas" style="height:{total}px">
    {#each visible as v (v.index)}
      <button
        class="row"
        class:active={v.index === activeIndex}
        class:selected={selected.has(v.item.path)}
        class:reject={v.item.flag === "reject"}
        style="transform:translateY({v.y}px)"
        onclick={(e) => onrowclick(e, v.index)}
        ondblclick={() => onrowdblclick(v.index)}
      >
        <span class="c-thumb"><Thumb item={v.item} size={320} /></span>
        <span class="c-name" title={v.item.path}>{v.item.name}</span>
        <span class="c-marks">
          {#if v.item.rating > 0}<span class="stars">{"★".repeat(v.item.rating)}</span>{/if}
          {#if v.item.label}<span class="dot" style="background:var({labelColor(v.item.label)})"></span>{/if}
          {#if v.item.flag === "pick"}<span class="fl pick">✓</span>{/if}
          {#if v.item.flag === "reject"}<span class="fl rej">✕</span>{/if}
          {#each v.item.tags as t}<span class="tag">{t}</span>{/each}
        </span>
        <span class="c-type">{v.item.kind === "image" ? v.item.ext.toUpperCase() : v.item.kind.toUpperCase()}</span>
        <span class="c-size">{fmtSize(v.item.size)}</span>
        <span class="c-date">{fmtDate(v.item.mtime)}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .head,
  .row {
    display: grid;
    grid-template-columns: 52px minmax(160px, 1fr) minmax(120px, 220px) 64px 84px 168px;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
  }
  .head {
    height: 30px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--text-faint);
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .vp {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--viewport-bg);
  }
  .canvas {
    position: relative;
    width: 100%;
  }
  .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 46px;
    width: 100%;
    text-align: left;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    color: var(--text);
    background: transparent;
  }
  .row:hover { background: var(--bg-hover); }
  .row.selected { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .row.active { background: color-mix(in srgb, var(--accent) 22%, transparent); box-shadow: inset 2px 0 0 var(--accent); }
  .row.reject { opacity: 0.5; }

  .c-thumb { width: 40px; height: 40px; display: flex; align-items: center; justify-content: center; overflow: hidden; border-radius: 4px; }
  .c-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
  .c-marks { display: flex; align-items: center; gap: 5px; overflow: hidden; }
  .c-type { color: var(--text-dim); font-size: 11.5px; }
  .c-size { color: var(--text-dim); font-size: 12px; text-align: right; }
  .c-date { color: var(--text-faint); font-size: 12px; }

  .stars { color: var(--star); font-size: 12px; }
  .dot { width: 11px; height: 11px; border-radius: 3px; }
  .fl { font-weight: 700; font-size: 12px; }
  .fl.pick { color: var(--pick); }
  .fl.rej { color: var(--reject); }
  .tag { font-size: 10.5px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 10px; padding: 0 7px; color: var(--text-dim); white-space: nowrap; }
</style>
