<script lang="ts">
  import { tt } from "../lib/i18n";
  import { api, type GuiSettings } from "../lib/api";

  export let isOpen = false;
  export let onClose: () => void = () => {};

  type Tab = "general" | "zerotrust" | "routing";
  let activeTab: Tab = "general";

  let settings: GuiSettings = {
    protocol: "masque",
    scan: "balanced",
    ip: "v4",
    noize: "firewall",
    quick_reconnect: true,
    socks: "127.0.0.1:1819",
    http_proxy: "",
    upstream: "",
    h2: false,
    fragment: false,
    language: "en",
    autoconnect: false,
    system_proxy: false,
    team: "",
    access_token: "",
    route_direct: "",
    route_block: "",
    auto_update: true,
    peer: "",
    wiw_outer: "",
    wiw_inner: "",
  };

  let loading = false;
  let savedMessage = false;
  let errorMessage: string | null = null;
  let showToken = false;

  async function loadSettings() {
    loading = true;
    errorMessage = null;
    try {
      const data = await api.getSettings();
      settings = { ...settings, ...data };
    } catch (e) {
      errorMessage = String(e);
    } finally {
      loading = false;
    }
  }

  async function save() {
    errorMessage = null;
    savedMessage = false;
    try {
      await api.saveSettings(settings);
      savedMessage = true;
      setTimeout(() => {
        savedMessage = false;
        onClose();
      }, 800);
    } catch (e) {
      errorMessage = String(e);
    }
  }

  function addChip(field: "route_direct" | "route_block", text: string) {
    const curr = settings[field] || "";
    const items = curr.split(/[,\s]+/).map(s => s.trim()).filter(Boolean);
    if (!items.includes(text)) {
      items.push(text);
      settings[field] = items.join(", ");
    }
  }

  $: if (isOpen) {
    loadSettings();
  }
</script>

{#if isOpen}
  <div
    class="modal-overlay"
    on:click|self={onClose}
    on:keydown={(e) => e.key === "Escape" && onClose()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="modal-box">
      <div class="modal-header">
        <h2>{$tt("settingsTitle")}</h2>
        <button class="close-btn" on:click={onClose} aria-label="Close">✕</button>
      </div>

      <div class="tabs">
        <button
          class="tab-btn"
          class:active={activeTab === "general"}
          on:click={() => (activeTab = "general")}
        >
          {$tt("tabGeneral")}
        </button>
        <button
          class="tab-btn"
          class:active={activeTab === "zerotrust"}
          on:click={() => (activeTab = "zerotrust")}
        >
          {$tt("tabZeroTrust")}
        </button>
        <button
          class="tab-btn"
          class:active={activeTab === "routing"}
          on:click={() => (activeTab = "routing")}
        >
          {$tt("tabRouting")}
        </button>
      </div>

      {#if loading}
        <div class="status-msg">{$tt("connecting")}</div>
      {:else}
        <div class="modal-content">
          {#if activeTab === "general"}
            <div class="form-group">
              <label class="toggle-row">
                <input type="checkbox" bind:checked={settings.auto_update} />
                <div class="toggle-text">
                  <strong>{$tt("autoUpdate")}</strong>
                  <span>{$tt("autoUpdateDesc")}</span>
                </div>
              </label>
            </div>

            <div class="form-group">
              <label class="toggle-row">
                <input type="checkbox" bind:checked={settings.autoconnect} />
                <div class="toggle-text">
                  <strong>{$tt("autoConnect")}</strong>
                </div>
              </label>
            </div>

            <div class="form-group">
              <label class="toggle-row">
                <input type="checkbox" bind:checked={settings.quick_reconnect} />
                <div class="toggle-text">
                  <strong>{$tt("quickReconnect")}</strong>
                  <span>{$tt("quickReconnectDesc")}</span>
                </div>
              </label>
            </div>

            <div class="form-row">
              <label>
                <strong>{$tt("protocol")}</strong>
                <select bind:value={settings.protocol}>
                  <option value="masque">MASQUE (HTTP/3 & HTTP/2)</option>
                  <option value="wg">WireGuard</option>
                  <option value="gool">gool (Nested WireGuard)</option>
                </select>
              </label>
              <label>
                <strong>{$tt("scanMode")}</strong>
                <select bind:value={settings.scan}>
                  <option value="balanced">Balanced</option>
                  <option value="turbo">Turbo</option>
                  <option value="thorough">Thorough</option>
                  <option value="stealth">Stealth</option>
                  <option value="ironclad">Ironclad</option>
                </select>
              </label>
            </div>

            <div class="form-row">
              <label>
                <strong>{$tt("ipVersion")}</strong>
                <select bind:value={settings.ip}>
                  <option value="v4">IPv4 Only</option>
                  <option value="v6">IPv6 Only</option>
                  <option value="dual">Dual Stack</option>
                </select>
              </label>
              <label>
                <strong>{$tt("noiseProfile")}</strong>
                <select bind:value={settings.noize}>
                  <option value="firewall">Firewall (High Obfuscation)</option>
                  <option value="balanced">Balanced</option>
                  <option value="aggressive">Aggressive</option>
                  <option value="light">Light</option>
                  <option value="off">Off</option>
                </select>
              </label>
            </div>
          {/if}

          {#if activeTab === "zerotrust"}
            <div class="info-banner">
              <strong>{$tt("zeroTrustTitle")}</strong>
              <p>{$tt("zeroTrustDesc")}</p>
            </div>

            <div class="form-group">
              <label>
                <strong>{$tt("teamName")}</strong>
                <input
                  type="text"
                  placeholder={$tt("teamPlaceholder")}
                  bind:value={settings.team}
                />
              </label>
            </div>

            <div class="form-group">
              <label>
                <strong>{$tt("accessToken")}</strong>
                <div class="token-input-row">
                  {#if showToken}
                    <input
                      type="text"
                      placeholder={$tt("accessTokenPlaceholder")}
                      bind:value={settings.access_token}
                    />
                  {:else}
                    <input
                      type="password"
                      placeholder={$tt("accessTokenPlaceholder")}
                      bind:value={settings.access_token}
                    />
                  {/if}
                  <button
                    type="button"
                    class="btn-secondary toggle-token"
                    on:click={() => (showToken = !showToken)}
                  >
                    {showToken ? $tt("hideToken") : $tt("showToken")}
                  </button>
                </div>
              </label>
            </div>
          {/if}

          {#if activeTab === "routing"}
            <div class="info-banner">
              <strong>{$tt("routingTitle")}</strong>
              <p>{$tt("routingDesc")}</p>
            </div>

            <div class="form-group">
              <label>
                <strong>{$tt("routeDirect")}</strong>
                <textarea
                  rows="3"
                  placeholder={$tt("routeDirectPlaceholder")}
                  bind:value={settings.route_direct}
                ></textarea>
              </label>
              <div class="chips">
                <button type="button" class="chip" on:click={() => addChip("route_direct", "private")}>+ private</button>
                <button type="button" class="chip" on:click={() => addChip("route_direct", "192.168.0.0/16")}>+ 192.168.0.0/16</button>
                <button type="button" class="chip" on:click={() => addChip("route_direct", "bank.ir")}>+ bank.ir</button>
              </div>
            </div>

            <div class="form-group">
              <label>
                <strong>{$tt("routeBlock")}</strong>
                <textarea
                  rows="3"
                  placeholder={$tt("routeBlockPlaceholder")}
                  bind:value={settings.route_block}
                ></textarea>
              </label>
              <div class="chips">
                <button type="button" class="chip" on:click={() => addChip("route_block", "ads.example.com")}>+ ads.example.com</button>
                <button type="button" class="chip" on:click={() => addChip("route_block", "port:25")}>+ port:25</button>
              </div>
            </div>

            <small class="help-text">{$tt("routingHelp")}</small>
          {/if}
        </div>
      {/if}

      {#if errorMessage}
        <div class="error-banner">{errorMessage}</div>
      {/if}
      {#if savedMessage}
        <div class="success-banner">{$tt("saved")}</div>
      {/if}

      <div class="modal-footer">
        <button class="btn-secondary" on:click={onClose}>{$tt("cancel")}</button>
        <button class="btn-primary" on:click={save}>{$tt("save")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.65);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 16px;
    backdrop-filter: blur(2px);
  }

  .modal-box {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--muted);
    border-radius: 12px;
    width: 100%;
    max-width: 460px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px;
    border-bottom: 1px solid rgba(128, 128, 128, 0.2);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--muted);
    font-size: 1.25rem;
    cursor: pointer;
    padding: 4px 8px;
  }

  .close-btn:hover {
    color: var(--fg);
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid rgba(128, 128, 128, 0.2);
    background: rgba(128, 128, 128, 0.05);
  }

  .tab-btn {
    flex: 1;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    color: var(--muted);
    padding: 10px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
  }

  .tab-btn.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    background: rgba(128, 128, 128, 0.08);
  }

  .modal-content {
    padding: 16px;
    overflow-y: auto;
    flex: 1;
  }

  .status-msg {
    padding: 24px;
    text-align: center;
    color: var(--muted);
  }

  .form-group {
    margin-bottom: 14px;
  }

  .form-row {
    display: flex;
    gap: 12px;
    margin-bottom: 14px;
  }

  .form-row label {
    flex: 1;
  }

  label strong {
    display: block;
    font-size: 0.85rem;
    margin-bottom: 6px;
  }

  input[type="text"],
  input[type="password"],
  select,
  textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: rgba(128, 128, 128, 0.08);
    color: var(--fg);
    font-size: 0.9rem;
  }

  textarea {
    resize: vertical;
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    cursor: pointer;
  }

  .toggle-row input[type="checkbox"] {
    margin-top: 3px;
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }

  .toggle-text strong {
    display: block;
    font-size: 0.9rem;
    margin: 0;
  }

  .toggle-text span {
    font-size: 0.78rem;
    color: var(--muted);
  }

  .token-input-row {
    display: flex;
    gap: 8px;
  }

  .toggle-token {
    padding: 6px 12px;
    font-size: 0.8rem;
    white-space: nowrap;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }

  .chip {
    display: inline-block;
    font-size: 0.75rem;
    padding: 2px 8px;
    background: rgba(128, 128, 128, 0.15);
    border-radius: 12px;
    cursor: pointer;
    color: var(--muted);
  }

  .chip:hover {
    background: var(--accent);
    color: #fff;
  }

  .info-banner {
    background: rgba(79, 124, 255, 0.1);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: 4px;
    margin-bottom: 14px;
    font-size: 0.82rem;
  }

  .info-banner p {
    margin: 4px 0 0 0;
    color: var(--muted);
  }

  .help-text {
    color: var(--muted);
    font-size: 0.75rem;
    display: block;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 16px;
    border-top: 1px solid rgba(128, 128, 128, 0.2);
  }

  .btn-primary {
    background: var(--accent);
    color: #fff;
    border: none;
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-secondary {
    background: transparent;
    color: var(--fg);
    border: 1px solid var(--muted);
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .success-banner {
    padding: 8px 16px;
    background: rgba(46, 204, 113, 0.15);
    color: #2ecc71;
    font-size: 0.85rem;
    text-align: center;
  }

  .error-banner {
    padding: 8px 16px;
    background: rgba(231, 76, 60, 0.15);
    color: var(--danger);
    font-size: 0.85rem;
    text-align: center;
  }
</style>
