//! aether-gui entry point: Tauri desktop shell, with a headless
//! `--check` dry-run kept for CI and debugging (no window needed).

use tauri::Manager;

use aether_gui::{commands, state::AppState};

// ---------------------------------------------------------------------------
// Tauri IPC commands: thin wrappers over `commands::*` (plan §2.4,
// adapted to the backend API — settings resolve inside each command).

#[tauri::command]
async fn snapshot(state: tauri::State<'_, AppState>) -> Result<commands::SnapshotDto, String> {
    Ok(commands::snapshot(&state).await)
}

#[tauri::command]
async fn get_settings() -> Result<aether::gui::GuiSettings, String> {
    let path = commands::default_settings_path();
    commands::get_settings(&path).await
}

#[tauri::command]
async fn save_settings(settings: aether::gui::GuiSettings) -> Result<(), String> {
    let path = commands::default_settings_path();
    commands::save_settings(&path, settings).await
}

#[tauri::command]
async fn provision(
    state: tauri::State<'_, AppState>,
) -> Result<aether::api::IdentitySummary, String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    commands::provision(&state, &settings).await
}

#[tauri::command]
async fn scan_once(
    state: tauri::State<'_, AppState>,
) -> Result<aether::api::Endpoint, String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    commands::scan_once(&state, &settings).await
}

#[tauri::command]
async fn verify_once(state: tauri::State<'_, AppState>, peer: String) -> Result<bool, String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    let addr = peer.parse().map_err(|e| format!("bad peer {peer}: {e}"))?;
    commands::verify_once(&state, &settings, addr).await
}

#[tauri::command]
async fn connect(
    state: tauri::State<'_, AppState>,
    peer: String,
    mode: Option<String>,
) -> Result<(), String> {
    if let Some(mode) = mode.as_deref() {
        commands::set_traffic_mode(&state, mode).await?;
    }
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    let addr = peer.parse().map_err(|e| format!("bad peer {peer}: {e}"))?;
    commands::connect(&state, &settings, addr).await
}

#[tauri::command]
async fn provision_gool(
    state: tauri::State<'_, AppState>,
) -> Result<(aether::api::IdentitySummary, Vec<String>), String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    commands::provision_gool(&state, &settings).await
}

#[tauri::command]
async fn connect_gool(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await?;
    commands::connect_gool_cmd(&state, &settings).await
}

#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    commands::disconnect(&state).await
}

#[tauri::command]
async fn get_traffic_mode(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(commands::get_traffic_mode(&state).await)
}

#[tauri::command]
async fn set_traffic_mode(state: tauri::State<'_, AppState>, mode: String) -> Result<String, String> {
    commands::set_traffic_mode(&state, &mode).await
}

#[tauri::command]
async fn drain_events(max: usize) -> Result<Vec<aether::events::ApiEvent>, String> {
    Ok(commands::drain_events(max).await)
}

#[tauri::command]
async fn drain_logs(max: usize, level: Option<String>) -> Result<Vec<String>, String> {
    let records = commands::drain_logs(max, level).await;
    Ok(records.into_iter().map(|r| r.message).collect())
}

#[tauri::command]
async fn diagnostics() -> Result<serde_json::Value, String> {
    let path = commands::default_settings_path();
    let settings = commands::get_settings(&path).await.unwrap_or_default();
    Ok(commands::diagnostics(&settings).await)
}

// ---------------------------------------------------------------------------
// Tray: Show/Hide/Quit only. Transport actions (Connect/Disconnect) stay
// in the main window until the async menu-event bridge lands — menu
// callbacks are sync and must never block on the tunnel.

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::TrayIconBuilder,
    };

    let Some(icon) = app.default_window_icon().cloned() else {
        log::warn!("no window icon available; tray disabled until `tauri icon` assets exist");
        return Ok(());
    };

    let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
    let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &hide, &quit]).build()?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "hide" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn run_tauri() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            snapshot,
            get_settings,
            save_settings,
            provision,
            provision_gool,
            scan_once,
            verify_once,
            connect,
            connect_gool,
            disconnect,
            get_traffic_mode,
            set_traffic_mode,
            drain_events,
            drain_logs,
            diagnostics
        ])
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running aether-gui");
}

// ---------------------------------------------------------------------------
// Headless dry-run (unchanged behavior): validates settings and prints
// the resolved scan/tunnel plan without opening a window.

async fn run_cli_check(args: Vec<String>) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("aether-gui — desktop GUI for Aether (also: settings dry-run)");
        println!();
        println!("Usage:");
        println!("  aether-gui                       open the GUI window");
        println!("  aether-gui [--settings <path>] [--check]");
        println!();
        println!("  --settings <path>  aether-gui.toml path (default: OS config dir)");
        println!("  --check            validate + print resolved scan/tunnel plan, then exit");
        return;
    }
    let mut settings_path = commands::default_settings_path();
    let mut check_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--settings" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    settings_path = p.clone();
                }
            }
            "--check" => check_only = true,
            other => {
                eprintln!("unknown option '{other}'; see --help");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let settings = match commands::get_settings(&settings_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("settings error ({settings_path}): {e}");
            std::process::exit(1);
        }
    };
    if !check_only {
        eprintln!("hint: run with no arguments to open the GUI, or add --check for a dry-run");
        std::process::exit(2);
    }
    match commands::scan_request_for(&settings) {
        Ok(req) => println!(
            "scan: transport={} mode={} ip={:?} wanted={} ports={}",
            req.transport.label(),
            req.mode,
            req.ip,
            req.wanted,
            req.ports.len()
        ),
        Err(e) => {
            eprintln!("scan plan error: {e}");
            std::process::exit(1);
        }
    }
    match commands::tunnel_spec_for(&settings) {
        Ok(spec) => println!(
            "tunnel: transport={} socks={} http={} gool_outer={} gool_inner={}",
            spec.transport.label(),
            spec.socks,
            spec.http.map(|a| a.to_string()).unwrap_or_else(|| "off".to_string()),
            spec.outer.map(|a| a.to_string()).unwrap_or_else(|| "-".to_string()),
            spec.inner.map(|a| a.to_string()).unwrap_or_else(|| "-".to_string()),
        ),
        Err(e) => {
            eprintln!("tunnel plan error: {e}");
            std::process::exit(1);
        }
    }
    if commands::needs_bind_confirm(&settings) {
        println!("warning: non-loopback bind needs the typed confirm in the GUI");
    }
    println!("settings OK ({settings_path})");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // NOTE: headless CI uses --check explicitly. Bare `aether-gui` with
    // no display aborts inside wry with a clear error.
    if args.is_empty() {
        run_tauri();
        return;
    }
    run_cli_check(args).await;
}
