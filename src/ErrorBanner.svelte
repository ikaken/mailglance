<script lang="ts">
  import { api, helpCode } from "./api";

  let { message }: { message: string } = $props();
  const code = $derived(helpCode(message));
</script>

<div class="error-banner">
  {message}
  {#if code}
    <button class="help-link" onclick={() => api.openHelp(code)}>対処方法を見る</button>
  {/if}
</div>

<style>
  .help-link {
    margin-left: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
