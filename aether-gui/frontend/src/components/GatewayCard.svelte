<script lang="ts">
  import { tt } from "../lib/i18n";

  export let phase: string = "idle";
  export let gateway: string | null = null;
  export let error: string | null = null;
</script>

<div class="card">
  <div><strong>{$tt("status")}:</strong> {phase}</div>
  {#if gateway}
    <div><strong>{$tt("gateway")}:</strong> <span dir="ltr">{gateway}</span></div>
  {/if}
  {#if error}
    <div><strong>Error:</strong> {error}</div>
  {/if}
</div>
