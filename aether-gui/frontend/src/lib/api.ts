/**
 * Backend RPC wrappers. Under Tauri these call `#[tauri::command]`s of the
 * same name; in a plain browser (mock/dev preview) they fall back gracefully.
 */

type Phase =
  | "idle" | "provisioning" | "scanning" | "verifying" | "connecting"
  | "connected" | "reconnecting" | "stopped" | "error";

export interface Snapshot {
  phase: Phase;
  gateway: string | null;
  error: string | null;
}

export interface GuiSettings {
  protocol: string;
  scan: string;
  ip: string;
  noize: string;
  quick_reconnect: boolean;
  socks: string;
  http_proxy?: string;
  upstream?: string;
  h2?: boolean;
  fragment?: boolean;
  language?: string;
  autoconnect?: boolean;
  system_proxy?: boolean;
  team?: string;
  access_token?: string;
  route_direct?: string;
  route_block?: string;
  auto_update?: boolean;
  peer?: string;
  wiw_outer?: string;
  wiw_inner?: string;
}

const defaultSettings: GuiSettings = {
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

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Lazy import keeps browser builds free of the Tauri runtime.
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(cmd, args);
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  getSettings: async (): Promise<GuiSettings> => {
    if (!isTauri()) {
      try {
        const saved = localStorage.getItem("aether.settings");
        return saved ? JSON.parse(saved) : defaultSettings;
      } catch {
        return defaultSettings;
      }
    }
    return invoke<GuiSettings>("get_settings");
  },
  saveSettings: async (settings: GuiSettings): Promise<void> => {
    if (!isTauri()) {
      try {
        localStorage.setItem("aether.settings", JSON.stringify(settings));
      } catch {
        // Storage unavailable
      }
      return;
    }
    return invoke<void>("save_settings", { settings });
  },
  provision: () => invoke<unknown>("provision"),
  scanOnce: () => invoke<{ ip: string; port: number; rtt_ms: number }>("scan_once"),
  verifyOnce: (peer: string) => invoke<boolean>("verify_once", { peer }),
  connect: (peer: string) => invoke<void>("connect", { peer }),
  disconnect: () => invoke<void>("disconnect"),
  drainEvents: (max: number) => invoke<unknown[]>("drain_events", { max }),
  drainLogs: (max: number, level?: string) =>
    invoke<string[]>("drain_logs", { max, level }),
  diagnostics: () => invoke<unknown>("diagnostics"),
  getTrafficMode: () => invoke<string>("get_traffic_mode"),
  setTrafficMode: (mode: string) => invoke<void>("set_traffic_mode", { mode }),
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
