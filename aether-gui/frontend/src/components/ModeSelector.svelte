<script lang="ts">
  import { tt } from "../lib/i18n";

  export let mode: string = "proxy";
  export let onchange: (mode: string) => void = () => {};
</script>

<div class="mode-selector">
  <button
    class:active={mode === "proxy"}
    on:click={() => onchange("proxy")}
    title={$tt("modeProxyDesc")}
  >
    {$tt("modeProxy")}
  </button>
  <button
    class:active={mode === "system"}
    on:click={() => onchange("system")}
    title={$tt("modeSystemDesc")}
  >
    {$tt("modeSystem")}
  </button>
  <button
    class:active={mode === "vpn"}
    on:click={() => onchange("vpn")}
    title={$tt("modeVpnDesc")}
  >
    {$tt("modeVpn")}
  </button>
</div>
