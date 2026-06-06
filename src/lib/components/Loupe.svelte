<script lang="ts">
  import { api } from "$lib/api";
  import type { MediaItem } from "$lib/types";

  let { item }: { item: MediaItem | null } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  // Re-resolve the large source whenever the active item changes.
  $effect(() => {
    const it = item;
    src = null;
    failed = false;
    if (!it) return;
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
    <!-- best-effort: H.264 plays everywhere, HEVC where the OS codec allows -->
    {#if src}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video {src} controls autoplay></video>
    {/if}
  {:else if failed}
    <div class="empty">
      Can't preview this file{item.kind === "other" ? " (unsupported format)" : ""}.
    </div>
  {:else if src}
    <img {src} alt={item.name} draggable="false" />
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
  .empty {
    color: var(--text-faint);
    font-size: 14px;
  }
</style>
