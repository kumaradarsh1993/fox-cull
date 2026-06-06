<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { settings } from "$lib/settings.svelte";
  let { children } = $props();

  onMount(() => {
    settings.init();
  });

  // Apply the chosen theme to <html data-theme>; reacts to live toggles.
  $effect(() => {
    document.documentElement.setAttribute("data-theme", settings.s.theme);
  });
</script>

{@render children()}
