<script lang="ts">
  // Lightroom-style background-activity chip (top-left, under the brand): one
  // slim progress bar for the primary job, a "+n" when more run concurrently,
  // and a click-to-expand list naming every operation — so "why is the disk
  // busy / when will the blur stop" is always answerable at a glance.
  import { activity, type Job } from "$lib/activity.svelte";

  let expanded = $state(false);

  let jobs = $derived(activity.list);
  let running = $derived(activity.running);
  let primary = $derived(running[0] ?? jobs[jobs.length - 1]);

  function pct(j: Job): number {
    return j.total > 0 ? Math.min(100, Math.round((j.done / j.total) * 100)) : 0;
  }
  function detail(j: Job): string {
    if (j.state === "error") return "failed";
    if (j.state === "done") return "done";
    return j.total > 0 ? `${j.done.toLocaleString()} / ${j.total.toLocaleString()}` : "working…";
  }
  // Collapse the detail list when everything finishes.
  $effect(() => {
    if (jobs.length === 0) expanded = false;
  });
</script>

{#if jobs.length}
  <div class="act">
    <button
      class="main"
      onclick={() => (expanded = !expanded)}
      title="Background activity — click for details"
    >
      <span class="lbl" class:err={primary.state === "error"}>{primary.label}</span>
      {#if running.length > 1}<span class="more">+{running.length - 1}</span>{/if}
      <span class="num">
        {#if primary.state === "running" && primary.total > 0}{pct(primary)}%{/if}
      </span>
      <span class="bar">
        <span
          class="fill"
          class:indet={primary.total === 0 && primary.state === "running"}
          class:err={primary.state === "error"}
          style="width:{primary.total > 0 || primary.state !== 'running' ? (primary.state === 'running' ? pct(primary) : 100) : 40}%"
        ></span>
      </span>
    </button>
    {#if expanded}
      <div class="list">
        {#each jobs as j (j.id)}
          <div class="job">
            <span class="jl" class:err={j.state === "error"} title={j.label}>{j.label}</span>
            <span class="jd">{detail(j)}</span>
            <span class="bar">
              <span
                class="fill"
                class:indet={j.total === 0 && j.state === "running"}
                class:err={j.state === "error"}
                style="width:{j.total > 0 || j.state !== 'running' ? (j.state === 'running' ? pct(j) : 100) : 40}%"
              ></span>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .act {
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    flex: 0 0 auto;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    width: 100%;
    padding: 6px 10px 7px;
    text-align: left;
  }
  .main:hover {
    background: var(--bg-hover);
  }
  .lbl {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--text-dim);
  }
  .more {
    flex: 0 0 auto;
    font-size: 10.5px;
    color: var(--accent);
    font-weight: 600;
  }
  .num {
    flex: 0 0 auto;
    font-size: 10.5px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .bar {
    flex: 0 0 100%;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--text-faint) 25%, transparent);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.25s ease;
  }
  .fill.err {
    background: var(--reject);
  }
  /* Indeterminate: a 40%-wide segment sweeping back and forth. */
  .fill.indet {
    animation: sweep 1.2s ease-in-out infinite alternate;
  }
  @keyframes sweep {
    from { transform: translateX(-30%); }
    to { transform: translateX(220%); }
  }

  .list {
    padding: 4px 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    border-top: 1px dashed var(--border);
  }
  .job {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .jl {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--text-dim);
  }
  .jl.err,
  .lbl.err {
    color: var(--reject);
  }
  .jd {
    flex: 0 0 auto;
    font-size: 10.5px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
</style>
