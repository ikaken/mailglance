<script lang="ts">
  import { paths, type IconName } from "./icons";

  let { name, size = 20 }: { name: IconName; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
  <path d={paths[name]} />
</svg>

<style>
  svg {
    flex-shrink: 0;
    display: block;
  }
</style>
