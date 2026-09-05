<script lang="ts">
  import { tt } from "../lib/i18n";

  export let lines: string[] = [];
</script>

<div class="card">
  <div><strong>{$tt("logs")}</strong></div>
  <pre class="logs" dir="ltr">{lines.join("\n")}</pre>
</div>
