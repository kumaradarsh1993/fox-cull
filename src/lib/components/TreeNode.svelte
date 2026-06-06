<script lang="ts">
  import { api } from "$lib/api";
  import type { TreeDir } from "$lib/types";

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

  async function toggle(e: MouseEvent) {
    e.stopPropagation();
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

<button
  class="trow"
  class:active={currentDir === node.path}
  style="padding-left:{6 + depth * 12}px"
  onclick={() => onselect(node.path)}
>
  {#if node.has_children}
    <span class="arrow" class:open onclick={toggle} role="button" tabindex="-1">▸</span>
  {:else}
    <span class="arrow-spacer"></span>
  {/if}
  <span class="tname" title={node.name}>{node.name}</span>
</button>

{#if open && kids}
  {#each kids as k (k.path)}
    <svelte:self node={k} {currentDir} {onselect} depth={depth + 1} />
  {/each}
{/if}

<style>
  .trow {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 4px 8px 4px 6px;
    text-align: left;
    border-radius: 5px;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.2;
  }
  .trow:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .trow.active {
    background: var(--accent-dim);
    color: #fff;
  }
  .arrow,
  .arrow-spacer {
    display: inline-block;
    width: 12px;
    flex: 0 0 12px;
    text-align: center;
    transition: transform 0.12s;
    color: var(--text-faint);
  }
  .arrow.open {
    transform: rotate(90deg);
  }
  .tname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
