<script lang="ts">
  import { api } from "$lib/api";
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem } from "$lib/types";

  let { item }: { item: MediaItem | null } = $props();

  let src = $state<string | null>(null); // sharp preview (image/raw) or video src
  let lowSrc = $state<string | null>(null); // cached grid thumb shown instantly
  let failed = $state(false);
  let videoErr = $state(false);

  // ── video trim state ──
  let vid = $state<HTMLVideoElement | null>(null);
  let dur = $state(0);
  let cur = $state(0);
  let inS = $state(0);
  let outS = $state<number | null>(null); // null = end
  let exporting = $state(false);
  let exportNote = $state<string | null>(null);

  $effect(() => {
    const it = item;
    src = null;
    lowSrc = null;
    failed = false;
    videoErr = false;
    dur = 0;
    cur = 0;
    inS = 0;
    outS = null;
    exportNote = null;
    if (!it) return;
    if (it.kind === "image" || it.kind === "raw") {
      loadThumb(it.path, 320).then((s) => {
        if (item === it && s && !src) lowSrc = s;
      });
    }
    if (it.kind === "video") {
      api.getTrim(it.path).then((t) => {
        if (item === it && t) {
          inS = t[0];
          outS = t[1];
        }
      });
    }
    (async () => {
      try {
        const p = await api.loupeSrc(it.path);
        if (item === it) src = api.fileSrc(p);
      } catch {
        if (item === it) failed = true;
      }
    })();
  });

  function onMeta() {
    if (vid) dur = vid.duration || 0;
  }
  function onTime() {
    if (vid) cur = vid.currentTime || 0;
  }
  function setIn() {
    inS = cur;
    if (outS != null && outS <= inS) outS = null;
    persist();
  }
  function setOut() {
    outS = cur;
    if (outS <= inS) inS = 0;
    persist();
  }
  function resetTrim() {
    inS = 0;
    outS = null;
    if (item) api.clearTrim(item.path);
    exportNote = null;
  }
  function persist() {
    if (item) api.setTrim(item.path, inS, outS ?? dur);
  }
  async function exportCut() {
    if (!item || exporting) return;
    const end = outS ?? dur;
    if (end <= inS) return;
    exporting = true;
    exportNote = "Cutting…";
    try {
      const out = await api.trimVideo(item.path, inS, end);
      exportNote = `Saved ${out.split(/[\\/]/).pop()}`;
      api.reveal(out);
    } catch (e) {
      exportNote = `Couldn't cut (${e})`;
    } finally {
      exporting = false;
    }
  }

  function fmt(s: number): string {
    if (!isFinite(s) || s < 0) s = 0;
    const m = Math.floor(s / 60);
    const sec = Math.floor(s % 60);
    return `${m}:${sec.toString().padStart(2, "0")}`;
  }
  let pct = (s: number) => (dur > 0 ? (s / dur) * 100 : 0);
  let canExport = $derived(dur > 0 && (outS ?? dur) > inS && (inS > 0 || (outS ?? dur) < dur));
</script>

<div class="loupe">
  {#if !item}
    <div class="empty">No selection</div>
  {:else if item.kind === "video"}
    {#if src && !videoErr}
      <div class="vwrap">
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          bind:this={vid}
          {src}
          controls
          autoplay
          onloadedmetadata={onMeta}
          ontimeupdate={onTime}
          onerror={() => (videoErr = true)}
        ></video>
        <div class="trim">
          <div class="track">
            <div class="range" style="left:{pct(inS)}%; right:{100 - pct(outS ?? dur)}%"></div>
            <div class="cursor" style="left:{pct(cur)}%"></div>
          </div>
          <div class="ctrls">
            <button onclick={setIn} title="Set in point to current time">⟤ In {fmt(inS)}</button>
            <button onclick={setOut} title="Set out point to current time">Out {fmt(outS ?? dur)} ⟥</button>
            <span class="len">cut {fmt((outS ?? dur) - inS)}</span>
            <span class="spacer"></span>
            {#if canExport}<button class="reset" onclick={resetTrim}>Reset</button>{/if}
            <button class="exp" onclick={exportCut} disabled={!canExport || exporting}>
              {exporting ? "Cutting…" : "✂ Export cut"}
            </button>
          </div>
          {#if exportNote}<div class="note">{exportNote}</div>{/if}
        </div>
      </div>
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
  .low {
    filter: blur(0.4px);
  }
  .empty {
    color: var(--text-faint);
    font-size: 14px;
  }

  .vwrap {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .vwrap video {
    flex: 1;
    min-height: 0;
    width: 100%;
  }
  .trim {
    flex: 0 0 auto;
    background: var(--bg-panel);
    border-top: 1px solid var(--border);
    padding: 8px 12px 10px;
  }
  .track {
    position: relative;
    height: 8px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--text-faint) 30%, transparent);
    margin-bottom: 8px;
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
    border-radius: 4px;
  }
  .cursor {
    position: absolute;
    top: -2px;
    width: 2px;
    height: 12px;
    background: #fff;
    transform: translateX(-1px);
  }
  .ctrls {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ctrls button {
    padding: 4px 10px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    font-size: 12.5px;
  }
  .ctrls button:hover {
    background: var(--bg-hover);
  }
  .len {
    color: var(--text-dim);
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
  .ctrls .exp {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
    font-weight: 600;
  }
  .ctrls .exp:disabled {
    opacity: 0.45;
  }
  .note {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-dim);
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
  .vfail .vt {
    color: var(--text-dim);
    font-weight: 600;
    font-size: 15px;
    margin: 0;
  }
  .vfail p {
    margin: 0;
    line-height: 1.5;
  }
  .obtn {
    margin-top: 4px;
    padding: 9px 16px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-on);
    font-size: 13.5px;
    font-weight: 600;
  }
  .obtn:hover {
    filter: brightness(1.06);
  }
</style>
