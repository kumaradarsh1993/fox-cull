<script lang="ts">
  import { loadThumb, loadVideoPoster } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item, size = 320 }: { item: MediaItem; size?: number } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  let isVideo = $derived(item.kind === "video");

  // Images/RAW → cached orientation-baked thumbnail. Videos → a real poster
  // frame extracted by the bundled ffmpeg (cached on the SSD). If the poster
  // isn't available yet (no ffmpeg, read-only cache on a Mac, undecodable clip)
  // we show the film placeholder instead of a blank cell.
  $effect(() => {
    const it = item;
    src = null;
    failed = false;
    if (it.kind === "other") return;
    let alive = true;
    const p = it.kind === "video" ? loadVideoPoster(it.path) : loadThumb(it.path, size);
    p.then((s) => {
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
    <img class="media" {src} alt={item.name} draggable="false" />
    {#if isVideo}<span class="play">▶</span>{/if}
  {:else if isVideo}
    <div class="ph vid">
      <span class="film">▶</span>
      <span class="vext">{item.ext.toUpperCase()}</span>
    </div>
  {:else if failed}
    <div class="ph">{item.kind === "raw" ? "RAW" : item.ext.toUpperCase()}</div>
  {:else}
    <div class="ph dim">·</div>
  {/if}
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
    background: color-mix(in srgb, var(--text-faint) 12%, var(--viewport-bg));
    overflow: hidden;
  }
  .media {
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
  .ph.dim { opacity: 0.4; font-size: 18px; }
  .ph.vid {
    flex-direction: column;
    gap: 5px;
    background: repeating-linear-gradient(
      45deg,
      color-mix(in srgb, var(--text-faint) 8%, var(--viewport-bg)),
      color-mix(in srgb, var(--text-faint) 8%, var(--viewport-bg)) 10px,
      color-mix(in srgb, var(--text-faint) 14%, var(--viewport-bg)) 10px,
      color-mix(in srgb, var(--text-faint) 14%, var(--viewport-bg)) 20px
    );
  }
  .ph.vid .film {
    font-size: 20px;
    color: var(--text);
    background: color-mix(in srgb, var(--text) 14%, transparent);
    width: 34px;
    height: 34px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-left: 3px;
  }
  .ph.vid .vext { font-size: 10px; font-weight: 700; color: var(--text-dim); letter-spacing: 0.5px; }
  .play {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    padding-left: 2px;
    pointer-events: none;
  }
  .badge {
    position: absolute;
    bottom: 4px;
    left: 4px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
  }
</style>
