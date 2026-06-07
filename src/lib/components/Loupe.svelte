<script lang="ts">
  import { api } from "$lib/api";
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item }: { item: MediaItem | null } = $props();

  let src = $state<string | null>(null); // sharp, capped preview
  let lowSrc = $state<string | null>(null); // cached grid thumb shown instantly
  let failed = $state(false);
  let videoErr = $state(false);

  // Re-resolve whenever the active item changes. Show the already-cached small
  // thumbnail at once (it's in memory — zero wait) so the photo never appears to
  // hang, then swap in the sharp preview the moment it's ready.
  $effect(() => {
    const it = item;
    src = null;
    lowSrc = null;
    failed = false;
    videoErr = false;
    if (!it) return;
    if (it.kind === "image" || it.kind === "raw") {
      loadThumb(it.path, 320).then((s) => {
        if (item === it && s && !src) lowSrc = s;
      });
    }
    (async () => {
      try {
        const p = await api.loupeSrc(it.path);
        // guard against a newer item having been selected mid-await
        if (item === it) src = api.fileSrc(p);
      } catch {
        if (item === it) failed = true;
      }
    })();
  });
</script>

<div class="loupe">
  {#if !item}
    <div class="empty">No selection</div>
  {:else if item.kind === "video"}
    <!-- best-effort: H.264 plays everywhere; HEVC (e.g. Osmo Pocket 3) plays on
         macOS and on Windows only with the HEVC extension. On a decode failure we
         fall back to opening the clip in the system player. -->
    {#if src && !videoErr}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video {src} controls autoplay onerror={() => (videoErr = true)}></video>
    {:else}
      <div class="empty vfail">
        <p class="vt">{item.name}</p>
        <p>This clip can't play in-app — likely HEVC/H.265 the webview can't decode.</p>
        <button class="obtn" onclick={() => item && api.openExternal(item.path)}>
          ▶ Open in system player
        </button>
      </div>
    {/if}
  {:else if failed}
    <div class="empty">
      Can't preview this file{item.kind === "other" ? " (unsupported format)" : ""}.
    </div>
  {:else if src}
    <img {src} alt={item.name} draggable="false" />
  {:else if lowSrc}
    <img class="low" src={lowSrc} alt={item.name} draggable="false" />
  {:else}
    <div class="empty">loading…</div>
  {/if}
</div>

<style>
  .loupe {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #0a0805;
    overflow: hidden;
  }
  img,
  video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  /* the instantly-shown cached thumb, upscaled — a touch of blur hides
     blockiness until the sharp preview swaps in */
  .low {
    filter: blur(0.4px);
  }
  .empty {
    color: var(--text-faint);
    font-size: 14px;
  }
  .vfail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    padding: 24px;
    max-width: 460px;
  }
  .vfail .vt { color: var(--text-dim); font-weight: 600; font-size: 15px; margin: 0; }
  .vfail p { margin: 0; line-height: 1.5; }
  .obtn {
    margin-top: 4px;
    padding: 9px 16px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-on);
    font-size: 13.5px;
    font-weight: 600;
  }
  .obtn:hover { filter: brightness(1.06); }
</style>
