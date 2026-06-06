<script lang="ts">
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item, size = 320 }: { item: MediaItem; size?: number } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  // Only visible cells are mounted (the grid/strip is virtualized), so we can
  // request on mount and let the loader cap concurrency + memoize. Videos and
  // unknown types never hit Rust — they render a placeholder.
  $effect(() => {
    const it = item;
    src = null;
    failed = false;
    if (it.kind === "video" || it.kind === "other") {
      failed = true;
      return;
    }
    let alive = true;
    loadThumb(it.path, size).then((s) => {
      if (!alive) return;
      if (s) src = s;
      else failed = true;
    });
    return () => {
      alive = false;
    };
  });
</script>

<div class="thumb">
  {#if src}
    <img {src} alt={item.name} draggable="false" />
  {:else if failed}
    <div class="ph">
      {#if item.kind === "video"}▶{:else if item.kind === "raw"}RAW{:else}{item.ext.toUpperCase()}{/if}
    </div>
  {:else}
    <div class="ph dim">·</div>
  {/if}
  {#if item.kind === "video"}<span class="badge">VIDEO</span>{/if}
  {#if item.kind === "raw"}<span class="badge">RAW</span>{/if}
</div>

<style>
  .thumb {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #0d0b08;
    overflow: hidden;
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .ph {
    color: var(--text-faint);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }
  .ph.dim {
    opacity: 0.4;
    font-size: 18px;
  }
  .badge {
    position: absolute;
    bottom: 4px;
    left: 4px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    padding: 1px 4px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.65);
    color: var(--text-dim);
  }
</style>
