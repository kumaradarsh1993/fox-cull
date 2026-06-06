<script lang="ts">
  import { api } from "$lib/api";
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item, size = 320 }: { item: MediaItem; size?: number } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  let isVideo = $derived(item.kind === "video");

  // Images/RAW: cached, orientation-baked thumbnail from Rust.
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

  // Video poster: show a real frame by seeking a muted <video> a touch past 0.
  // No canvas (avoids cross-origin taint); works for any codec the webview
  // can decode (H.264 everywhere, HEVC where the OS supports it).
  function seekToFrame(e: Event) {
    const v = e.currentTarget as HTMLVideoElement;
    try {
      v.currentTime = Math.min(0.5, (v.duration || 1) / 2);
    } catch {
      /* ignore */
    }
  }
</script>

<div class="thumb">
  {#if isVideo}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      class="media"
      src={api.fileSrc(item.path)}
      muted
      playsinline
      preload="metadata"
      onloadeddata={seekToFrame}
    ></video>
    <span class="badge">▶</span>
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
