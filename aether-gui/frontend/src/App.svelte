<script lang="ts">
  import { onMount } from "svelte";
  import { lang, tt, type Lang } from "./lib/i18n";
  import { api, isTauri } from "./lib/api";
  import ConnectButton from "./components/ConnectButton.svelte";
  import GatewayCard from "./components/GatewayCard.svelte";
  import ModeSelector from "./components/ModeSelector.svelte";
  import LiveLogs from "./components/LiveLogs.svelte";
  import SettingsModal from "./components/SettingsModal.svelte";

  let phase = "idle";
  let gateway: string | null = null;
  let error: string | null = null;
  let lines: string[] = [];
  let mode = "proxy";
  let showSettings = false;
  try {
    const saved = localStorage.getItem("aether.trafficMode");
    if (saved === "proxy" || saved === "system" || saved === "vpn") {
      mode = saved;
    }
  } catch {
    // Non-browser or storage unavailable: keep default.
  }

  async function refresh() {
    try {
      const snap = await api.snapshot();
      phase = snap.phase;
      gateway = snap.gateway;
      error = snap.error;
      const logs = await api.drainLogs(200);
      if (logs.length > 0) {
        lines = [...lines.slice(-400), ...logs];
      }
    } catch {
      // Not under Tauri (plain browser preview): stay idle.
    }
  }

  async function onConnect() {
    try {
      await api.provision();
      const ep = await api.scanOnce();
      const peer = `${ep.ip}:${ep.port}`;
      const ok = await api.verifyOnce(peer);
      if (ok) {
        await api.connect(peer);
      }
    } catch (e) {
      error = String(e);
    }
    await refresh();
  }

  async function onDisconnect() {
    try {
      await api.disconnect();
    } catch (e) {
      error = String(e);
    }
    await refresh();
  }

  function setLang(l: Lang) {
    lang.set(l);
  }

  async function setMode(next: string) {
    const prev = mode;
    mode = next;
    try {
      localStorage.setItem("aether.trafficMode", next);
    } catch {
      // Storage unavailable: UI state still updates.
    }
    try {
      await api.setTrafficMode(next);
    } catch {
      // Backend command missing (or backend unreachable): revert UI.
      mode = prev;
      try {
        localStorage.setItem("aether.trafficMode", prev);
      } catch {
        // Ignore storage errors on revert.
      }
    }
  }

  onMount(() => {
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => clearInterval(timer);
  });
</script>

<main>
  <header class="top-bar">
    <div class="lang-switch">
      <button class="small-btn" on:click={() => setLang("en")}>EN</button>
      <button class="small-btn" on:click={() => setLang("fa")}>فا</button>
    </div>
    <div class="actions">
      <button class="icon-btn" on:click={() => (showSettings = true)} title={$tt("settings")}>
        ⚙️ {$tt("settings")}
      </button>
    </div>
  </header>
  {#if !isTauri()}
    <div class="preview-notice">(browser preview — mock storage active)</div>
  {/if}

  <ModeSelector {mode} onchange={setMode} />
  <GatewayCard {phase} {gateway} {error} />
  <ConnectButton {phase} {onConnect} {onDisconnect} />
  <LiveLogs {lines} />

  <SettingsModal isOpen={showSettings} onClose={() => (showSettings = false)} />
</main>

<style>
  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .lang-switch {
    display: flex;
    gap: 6px;
  }

  .small-btn {
    padding: 4px 10px;
    font-size: 0.8rem;
    border-radius: 6px;
    background: rgba(128, 128, 128, 0.15);
    border: 1px solid var(--muted);
    color: var(--fg);
    cursor: pointer;
  }

  .small-btn:hover {
    background: var(--accent);
    color: #fff;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    font-size: 0.85rem;
    border-radius: 6px;
    background: rgba(128, 128, 128, 0.15);
    border: 1px solid var(--muted);
    color: var(--fg);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--accent);
    color: #fff;
  }

  .preview-notice {
    font-size: 0.75rem;
    color: var(--muted);
    margin-bottom: 8px;
    text-align: center;
  }
</style>
