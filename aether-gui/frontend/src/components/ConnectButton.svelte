<script lang="ts">
  import { tt } from "../lib/i18n";

  export let phase: string = "idle";
  export let onConnect: () => void = () => {};
  export let onDisconnect: () => void = () => {};

  $: busy = phase === "provisioning" || phase === "scanning" || phase === "verifying" || phase === "connecting";
  $: connected = phase === "connected";
</script>

{#if connected}
  <button on:click={onDisconnect}>{$tt("disconnect")}</button>
{:else}
  <button disabled={busy} on:click={onConnect}>
    {busy ? $tt("connecting") : $tt("connect")}
  </button>
{/if}
