<script lang="ts">
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item, size = 320 }: { item: MediaItem; size?: number } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  let isVideo = $derived(item.kind === "video");

  // Images/RAW: cached, orientation-baked thumbnail from Rust.
  // Videos render a lightweight film placeholder — we deliberately DON'T mount a
  // live <video> per cell (it forced the webview to decode every clip on scroll,
  // which was janky and showed blank frames for HEVC, e.g. the Osmo footage).
  // Real poster frames are a server-side (ffmpeg) job, tracked for later.
  $effect(() => {
    const it = item;
    src = null;
    failed = false;
    if (it.kind === "video" || it.kind === "other") return;
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
  {#if isVideo}
    <div class="ph vid">
      <span class="film">▶</span>
      <span class="vext">{item.ext.toUpperCase()}</span>
    </div>
  {:else if src}
    <img class="media" {src} alt={item.name} draggable="false" />
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
