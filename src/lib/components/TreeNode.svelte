<script lang="ts">
  import { api } from "$lib/api";
  import type { TreeDir } from "$lib/types";
  import Self from "./TreeNode.svelte";

  let {
    node,
    currentDir,
    onselect,
    depth = 0,
  }: {
    node: TreeDir;
    currentDir: string | null;
    onselect: (path: string) => void;
    depth?: number;
  } = $props();

  let open = $state(false);
  let kids = $state<TreeDir[] | null>(null);
  let loading = $state(false);

  async function toggle() {
    open = !open;
    if (open && kids === null) {
      loading = true;
      try {
        kids = await api.listTree(node.path);
      } catch {
        kids = [];
      }
      loading = false;
    }
  }
</script>

<div class="trow" class:active={currentDir === node.path} style="padding-left:{4 + depth * 14}px">
  {#if node.has_children}
    <button
      class="chev"
      class:open
      onclick={toggle}
      aria-label={open ? "Collapse" : "Expand"}
      title={open ? "Collapse" : "Expand"}
    >
      {open ? "▾" : "▸"}
    </button>
  {:else}
    <span class="chev-spacer"></span>
  {/if}
  <button class="tname" title={node.path} onclick={() => onselect(node.path)}>
    <span class="ico">{open ? "📂" : "📁"}</span>
    <span class="label">{node.name}</span>
  </button>
</div>

{#if open && kids}
  {#each kids as k (k.path)}
    <Self node={k} {currentDir} {onselect} depth={depth + 1} />
  {/each}
{/if}

<style>
  .trow {
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    border-radius: 6px;
  }
  .trow.active {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .trow.active .label {
    color: var(--accent);
    font-weight: 600;
  }

  /* Bigger, obvious expand/collapse target — clearly separate from the row's
     select action, with a filled triangle that flips on state. */
  .chev {
    flex: 0 0 auto;
    width: 24px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--text-faint);
    border-radius: 5px;
  }
  .chev:hover {
    background: var(--bg-hover);
    color: var(--accent);
  }
  .chev-spacer {
    flex: 0 0 auto;
    width: 24px;
  }

  .tname {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 6px;
    text-align: left;
    border-radius: 5px;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.2;
  }
  .tname:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .ico {
    font-size: 13px;
    flex: 0 0 auto;
    opacity: 0.85;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
