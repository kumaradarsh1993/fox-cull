<script lang="ts">
  import { api } from "$lib/api";
  import { activity } from "$lib/activity.svelte";
  import { loadThumb } from "$lib/thumbnail-loader";
  import type { MediaItem, FilmstripInfo } from "$lib/types";

  let { item }: { item: MediaItem | null } = $props();

  // Image transitions: the PREVIOUS photo stays painted until the next sharp
  // preview is fully decoded, then we swap in one frame — no black gap, and no
  // blur flash when flipping through an already-prepared folder. The blur-up
  // placeholder only appears when a load is genuinely slow (cold cache / heavy
  // file), where it's useful feedback instead of an artifact.
  let curSrc = $state<string | null>(null); // sharp image currently painted
  let lowSrc = $state<string | null>(null); // blurred placeholder (slow loads only)
  let showLow = $state(false);
  let vsrc = $state<string | null>(null); // video src (originals play directly)
  let failed = $state(false);
  let videoErr = $state(false);
  let epoch = 0; // bumps on every item change; stale async work checks it
  const SLOW_MS = 180; // how long a sharp load may take before we blur-up

  // ── H.264 proxy playback (clips the webview can't decode) ──
  let usingProxy = $state(false); // currently playing the converted preview
  let converting = $state(false);
  let proxyNote = $state<string | null>(null);
  // Live transcode progress, fed by the backend's activity events.
  let proxyPct = $derived.by(() => {
    const j = item ? activity.jobs[`proxy:${item.path}`] : undefined;
    return j && j.state === "running" && j.total > 0
      ? `${Math.round((j.done / j.total) * 100)}%`
      : "";
  });

  /** One-time ffmpeg convert to a cached H.264 preview, then play that. */
  async function convertAndPlay() {
    if (!item || converting) return;
    converting = true;
    proxyNote = null;
    const my = epoch;
    try {
      const p = await api.videoProxy(item.path);
      if (my === epoch) {
        usingProxy = true;
        videoErr = false;
        vsrc = api.fileSrc(p);
      }
    } catch (e) {
      if (my === epoch) proxyNote = `Couldn't convert this clip (${e})`;
    } finally {
      converting = false;
    }
  }

  // ── video trim state ──
  let vid = $state<HTMLVideoElement | null>(null);
  let paused = $state(false); // mirrors the element (autoplay starts playing)
  let dur = $state(0);
  let cur = $state(0);
  let inS = $state(0);
  let outS = $state<number | null>(null); // null = end
  let exporting = $state(false);
  let exportNote = $state<string | null>(null);

  // ── filmstrip scrub state ──
  let strip = $state<FilmstripInfo | null>(null);
  let stripSrc = $derived(strip ? api.fileSrc(strip.src) : null);
  let preview = $state<number | null>(null); // fraction 0..1 to preview, or null
  let scrubbing = $state(false);
  let trackEl = $state<HTMLDivElement | null>(null);
  const PREVIEW_W = 200;
  let previewH = $derived(
    strip ? Math.round((PREVIEW_W * strip.tile_h) / strip.tile_w) : 0,
  );

  $effect(() => {
    const it = item;
    const my = ++epoch;
    failed = false;
    videoErr = false;
    usingProxy = false;
    converting = false;
    proxyNote = null;
    paused = false;
    dur = 0;
    cur = 0;
    inS = 0;
    outS = null;
    exportNote = null;
    strip = null;
    preview = null;
    scrubbing = false;
    showLow = false;
    lowSrc = null;
    if (!it) {
      curSrc = null;
      vsrc = null;
      return;
    }
    if (it.kind === "video") {
      curSrc = null;
      vsrc = null;
      api.getTrim(it.path).then((t) => {
        if (my === epoch && t) {
          inS = t[0];
          outS = t[1];
        }
      });
      // Build/fetch the scrub filmstrip (lazy, cached on the SSD). Failure just
      // leaves the timeline as a plain seek bar with no frame preview.
      api
        .videoFilmstrip(it.path)
        .then((f) => {
          if (my === epoch) strip = f;
        })
        .catch(() => {});
      api
        .loupeSrc(it.path)
        .then((p) => {
          if (my === epoch) vsrc = api.fileSrc(p);
        })
        .catch(() => {
          if (my === epoch) failed = true;
        });
      return;
    }
    if (it.kind === "other") {
      curSrc = null;
      vsrc = null;
      failed = true;
      return;
    }
    // Image/RAW. Keep the previous photo painted; swap only when the new sharp
    // preview is DECODED (img.decode), so the swap is a single clean frame. If
    // the sharp load is slow (cold cache), fall back to the classic blur-up so
    // the user still gets instant feedback.
    vsrc = null;
    let sharpDone = false;
    const slow = setTimeout(() => {
      if (my !== epoch || sharpDone) return;
      loadThumb(it.path, 320).then((s) => {
        if (my === epoch && !sharpDone && s) {
          lowSrc = s;
          showLow = true;
          curSrc = null; // drop the stale previous photo under the placeholder
        }
      });
    }, SLOW_MS);
    (async () => {
      try {
        const p = await api.loupeSrc(it.path);
        if (my !== epoch) return;
        const url = api.fileSrc(p);
        const img = new Image();
        img.decoding = "async";
        img.src = url;
        try {
          await img.decode();
        } catch {
          /* decode() can reject for valid images — paint anyway */
        }
        if (my !== epoch) return;
        sharpDone = true;
        curSrc = url;
        showLow = false;
        lowSrc = null;
      } catch {
        if (my === epoch) {
          curSrc = null;
          failed = true;
        }
      }
    })();
    return () => clearTimeout(slow);
  });

  function onMeta() {
    if (vid) dur = vid.duration || 0;
  }

  // ── playback (exposed to the page's global key handler) ──
  export function togglePlay() {
    if (!vid) return;
    if (vid.paused) vid.play().catch(() => {});
    else vid.pause();
  }
  export function seekBy(d: number) {
    if (!vid) return;
    const max = dur || strip?.duration || vid.duration || 0;
    let t = vid.currentTime + d;
    if (t < 0) t = 0;
    if (max > 0 && t > max) t = max;
    vid.currentTime = t;
  }

  // ── timeline scrub: hover previews a frame, drag seeks the real video ──
  function fracFromEvent(e: PointerEvent): number {
    if (!trackEl) return 0;
    const r = trackEl.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }
  function seekTo(frac: number) {
    const d = dur || strip?.duration || 0;
    if (vid && d > 0) vid.currentTime = frac * d;
  }
  function onTrackDown(e: PointerEvent) {
    scrubbing = true;
    try {
      trackEl?.setPointerCapture(e.pointerId);
    } catch {}
    const f = fracFromEvent(e);
    preview = f;
    seekTo(f);
  }
  function onTrackMove(e: PointerEvent) {
    const f = fracFromEvent(e);
    preview = f;
    if (scrubbing) seekTo(f);
  }
  function onTrackUp(e: PointerEvent) {
    if (!scrubbing) return;
    scrubbing = false;
    try {
      trackEl?.releasePointerCapture(e.pointerId);
    } catch {}
  }
  function onTrackLeave() {
    if (!scrubbing) preview = null;
  }
  /** Sprite background-position (%) for the frame nearest `frac`. */
  function cellPos(frac: number): { x: number; y: number } {
    if (!strip) return { x: 0, y: 0 };
    const idx = Math.min(
      strip.count - 1,
      Math.max(0, Math.round(frac * (strip.count - 1))),
    );
    const col = idx % strip.cols;
    const row = Math.floor(idx / strip.cols);
    return {
      x: strip.cols > 1 ? (col / (strip.cols - 1)) * 100 : 0,
      y: strip.rows > 1 ? (row / (strip.rows - 1)) * 100 : 0,
    };
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
    {#if videoErr}
      <div class="empty vfail">
        <p class="vt">{item.name}</p>
        <p>This clip can't play in-app — likely HEVC/H.265 this machine has no codec for.</p>
        <button class="obtn" onclick={convertAndPlay} disabled={converting}>
          {converting ? `⏳ Converting…${proxyPct ? ` ${proxyPct}` : ""}` : "▶ Convert & play here"}
        </button>
        <p class="subnote">
          One-time: the bundled ffmpeg makes an H.264 preview, cached on the drive.
          The original file is never touched.
        </p>
        <button class="obtn ghost" onclick={() => item && api.openExternal(item.path)}>
          Open in system player instead
        </button>
        {#if proxyNote}<p class="subnote err">{proxyNote}</p>{/if}
      </div>
    {:else if vsrc}
      <div class="vwrap">
        <!-- svelte-ignore a11y_media_has_caption -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <video
          bind:this={vid}
          src={vsrc}
          autoplay
          onclick={togglePlay}
          onloadedmetadata={onMeta}
          ontimeupdate={onTime}
          onplay={() => (paused = false)}
          onpause={() => (paused = true)}
          onerror={() => {
            // Only a REAL decode/format failure shows the fallback card. An
            // aborted load (we switched clips mid-load) also fires `error` —
            // treating that as failure flashed the "can't play HEVC" card for
            // every clip that was actually about to play fine.
            const code = vid?.error?.code;
            if (!code || code === MediaError.MEDIA_ERR_ABORTED) return;
            // If a converted H.264 preview is already cached for this clip,
            // switch to it silently instead of asking again.
            if (item && !usingProxy) {
              const my = epoch;
              const p = item.path;
              api.videoProxyCached(p).then((cached) => {
                if (my !== epoch) return;
                if (cached) {
                  usingProxy = true;
                  vsrc = api.fileSrc(cached);
                } else {
                  videoErr = true;
                }
              });
            } else {
              videoErr = true;
            }
          }}
        ></video>
        {#if usingProxy}
          <span class="proxytag" title="The original couldn't decode in-app; you're watching the cached H.264 conversion. Trim still cuts the original.">converted preview</span>
        {/if}
        <div class="trim">
          <div class="playrow">
            <button class="pp" onclick={togglePlay} title={paused ? "Play (Space)" : "Pause (Space)"}>
              {paused ? "▶" : "⏸"}
            </button>
            <span class="time">{fmt(cur)} <span class="sep">/</span> {fmt(dur)}</span>
            <span class="spacer"></span>
            <span class="khint">Space play · Shift+← → seek</span>
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="track"
            class:scrubbing
            bind:this={trackEl}
            onpointerdown={onTrackDown}
            onpointermove={onTrackMove}
            onpointerup={onTrackUp}
            onpointerleave={onTrackLeave}
          >
            <div class="range" style="left:{pct(inS)}%; right:{100 - pct(outS ?? dur)}%"></div>
            <div class="cursor" style="left:{pct(cur)}%"></div>
            {#if preview != null && strip && stripSrc}
              {@const c = cellPos(preview)}
              <div
                class="scrubprev"
                style="left:{preview * 100}%; width:{PREVIEW_W}px; height:{previewH}px;
                       background-image:url('{stripSrc}');
                       background-size:{strip.cols * 100}% {strip.rows * 100}%;
                       background-position:{c.x}% {c.y}%;"
              >
                <span class="ts">{fmt(preview * (dur || strip.duration))}</span>
              </div>
            {/if}
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
      <!-- src still resolving (an IPC round-trip) — stay quietly black. The old
           code showed the HEVC-failure card here, flashing it before EVERY clip. -->
      <div class="empty"></div>
    {/if}
  {:else if failed}
    <div class="empty">
      Can't preview this file{item.kind === "other" ? " (unsupported format)" : ""}.
    </div>
  {:else}
    <!-- The previous sharp photo stays painted until the next one has decoded,
         then swaps in a single frame — no fade, no glow, no black gap. The
         blurred placeholder appears only for genuinely slow (cold) loads. -->
    <div class="stage">
      {#if showLow && lowSrc}
        <img class="layer ph" src={lowSrc} alt="" draggable="false" />
      {/if}
      {#if curSrc}
        <img class="layer hi" src={curSrc} alt={item.name} draggable="false" />
      {/if}
    </div>
  {/if}
</div>

<style>
  .loupe {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    /* Near-black NEUTRAL in every theme: the Focus surround is the reference
       your eye judges the photo's colors against, so it never takes the UI
       theme's tint (the old #0a0805 had a warm cast). */
    background: #0c0b0a;
    overflow: hidden;
  }
  img,
  video {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .stage {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  /* low-res placeholder for slow loads: softened, edges clipped by the stage.
     No opacity transitions anywhere — fades between layers were the "glow at
     the edges" artifact when flipping through warm photos. Swaps are instant. */
  .ph {
    filter: blur(10px);
    transform: scale(1.03); /* mask blurred edges bleeding past the frame */
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
  .playrow {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 7px;
  }
  .playrow .pp {
    width: 34px;
    height: 30px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--text);
    font-size: 13px;
    line-height: 1;
  }
  .playrow .pp:hover {
    background: var(--bg-hover);
  }
  .playrow .time {
    font-size: 12.5px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .playrow .time .sep {
    color: var(--text-faint);
    margin: 0 1px;
  }
  .playrow .khint {
    font-size: 11px;
    color: var(--text-faint);
  }
  .track {
    position: relative;
    height: 12px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text-faint) 30%, transparent);
    margin-bottom: 8px;
    cursor: pointer;
    touch-action: none; /* let pointer-drag scrub instead of scrolling */
  }
  .track.scrubbing {
    cursor: grabbing;
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
    border-radius: 6px;
    pointer-events: none;
  }
  .cursor {
    position: absolute;
    top: -2px;
    width: 2px;
    height: 16px;
    background: #fff;
    transform: translateX(-1px);
    pointer-events: none;
  }
  /* Floating frame preview shown under the scrub cursor (sprite cell). */
  .scrubprev {
    position: absolute;
    bottom: calc(100% + 9px);
    transform: translateX(-50%);
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    background-color: #000;
    background-repeat: no-repeat;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
    pointer-events: none;
    overflow: hidden;
    z-index: 60;
  }
  .scrubprev .ts {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    text-align: center;
    font-size: 11px;
    line-height: 1.5;
    color: #fff;
    background: rgba(0, 0, 0, 0.55);
    font-variant-numeric: tabular-nums;
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
  .obtn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .obtn.ghost {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-weight: 500;
  }
  .vfail .subnote {
    margin: 0;
    font-size: 12px;
    color: var(--text-faint);
    line-height: 1.5;
  }
  .vfail .subnote.err {
    color: var(--reject);
  }
  .vwrap {
    position: relative;
  }
  .proxytag {
    position: absolute;
    top: 10px;
    right: 12px;
    padding: 3px 9px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    color: rgba(255, 255, 255, 0.85);
    font-size: 11px;
    pointer-events: auto;
    z-index: 5;
  }
</style>
