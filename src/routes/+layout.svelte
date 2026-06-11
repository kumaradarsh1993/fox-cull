<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { settings } from "$lib/settings.svelte";
  import { activity } from "$lib/activity.svelte";
  let { children } = $props();

  onMount(() => {
    settings.init();
    activity.init(); // start listening for backend `activity` events
  });

  // Apply the chosen theme to <html data-theme>; reacts to live toggles.
  $effect(() => {
    document.documentElement.setAttribute("data-theme", settings.s.theme);
  });
</script>

{@render children()}
