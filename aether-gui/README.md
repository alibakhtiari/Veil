# aether-gui — Tauri v2 desktop app (GUI_PLAN.md §3)

## Layout (matches the master plan)

```text
aether-gui/
  src-tauri/                  # Rust backend (this crate) + Tauri config
    Cargo.toml                # aether = { path = "../../aether" }
    build.rs                  # no-op until Tauri adoption (see file)
    tauri.conf.json           # bundle config (needs icons/ + frontend/dist at bundle time)
    capabilities/default.json # least-privilege ACL
    icons/                    # generated via `tauri icon` (README inside)
    src/
      main.rs                 # --check dry-run today; Tauri entry at adoption
      lib.rs
      state.rs                # ConnectionPhase machine + AppState
      commands.rs             # provision/scan/verify/connect/disconnect/drains/diagnostics
      proxy.rs                # system-proxy builders (Win reg / macOS networksetup / GNOME)
      autostart.rs            # login-item builders (desktop file / Run key / plist)
  frontend/                   # Svelte 5 + Vite + EN/FA (RTL) — builds TODAY
```

## Use (no Tauri/WebView needed yet)

```sh
cargo run --manifest-path aether-gui/src-tauri/Cargo.toml -- --check
cargo test --manifest-path aether-gui/src-tauri/Cargo.toml   # 23 tests
cd aether-gui/frontend && npm install && npm run build && npm run check
```

## Tauri adoption (NEXT-STEPS.md §C)

`cargo tauri init` was deliberately NOT run (needs no new deps until
then). Adoption checklist: add `tauri` + `tauri-build` deps, flip
`build.rs` to `tauri_build::build()`, move `main.rs` to the Tauri
entry (keep the `--check` path for headless CI), annotate
`commands::*` with `#[tauri::command]`, generate icons.
