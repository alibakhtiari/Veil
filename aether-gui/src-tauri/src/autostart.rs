//! OS autostart integration (GUI_PLAN.md §3.2 / NEXT-STEPS.md §C).
//!
//! Same split as [`crate::proxy`]: pure builders (unit-tested) plus
//! thin impure `enable`/`disable` wrappers taking explicit paths, so
//! tests use temp dirs and never touch the real autostart locations.

use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Veil";
pub const APP_ID: &str = "dev.veil.app";

// ------------------------------------------------------------------ Linux

/// XDG autostart `.desktop` file contents.
pub fn linux_desktop_entry(exec_path: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={APP_NAME}\n\
         Comment=Censorship circumvention client\n\
         Exec={exec_path} --minimized\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Network;\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

pub fn linux_autostart_path(home: &Path) -> PathBuf {
    home.join(".config").join("autostart").join(format!("{APP_ID}.desktop"))
}

// ---------------------------------------------------------------- Windows

/// `reg add` argv registering the Run key (current user, no elevation).
pub fn win_enable_argv(exec_path: &str) -> Vec<String> {
    vec![
        "reg".to_string(),
        "add".to_string(),
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run".to_string(),
        "/v".to_string(),
        APP_NAME.to_string(),
        "/t".to_string(),
        "REG_SZ".to_string(),
        "/d".to_string(),
        format!("\"{exec_path}\" --minimized"),
        "/f".to_string(),
    ]
}

/// `reg delete` argv removing the Run key (missing key is not an error).
pub fn win_disable_argv() -> Vec<String> {
    vec![
        "reg".to_string(),
        "delete".to_string(),
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run".to_string(),
        "/v".to_string(),
        APP_NAME.to_string(),
        "/f".to_string(),
    ]
}

// ------------------------------------------------------------------ macOS

/// LaunchAgents plist contents (user agent, runs at login).
pub fn mac_plist_contents(label: &str, exec_path: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{exec_path}</string>\n\
         \t\t<string>--minimized</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         </dict>\n\
         </plist>\n"
    )
}

pub fn mac_plist_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{APP_ID}.plist"))
}

pub fn mac_label() -> String {
    format!("{APP_ID}.autostart")
}

// ------------------------------------------------------------------ apply

/// Write the Linux autostart entry (creates parent dirs).
pub fn linux_enable(home: &Path, exec_path: &str) -> std::io::Result<PathBuf> {
    let path = linux_autostart_path(home);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, linux_desktop_entry(exec_path))?;
    Ok(path)
}

pub fn linux_disable(home: &Path) -> std::io::Result<()> {
    let path = linux_autostart_path(home);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn linux_enabled(home: &Path) -> bool {
    linux_autostart_path(home).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aether-autostart-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp home");
        dir
    }

    #[test]
    fn linux_entry_is_a_valid_desktop_file_shape() {
        let entry = linux_desktop_entry("/opt/aether/aether-gui");
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("Exec=/opt/aether/aether-gui --minimized\n"));
        assert!(entry.contains("X-GNOME-Autostart-enabled=true"));
    }

    #[test]
    fn linux_enable_disable_roundtrip_in_tmp_home() {
        let home = tmp_home("roundtrip");
        assert!(!linux_enabled(&home));
        let path = linux_enable(&home, "/opt/aether/aether-gui").expect("enable");
        assert!(linux_enabled(&home));
        assert!(path.starts_with(&home));
        linux_disable(&home).expect("disable");
        assert!(!linux_enabled(&home));
        // Disabling twice is not an error.
        linux_disable(&home).expect("second disable");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn windows_run_key_argv_names_the_app() {
        let argv = win_enable_argv("C:\\Program Files\\Aether\\aether-gui.exe");
        assert!(argv.contains(&APP_NAME.to_string()));
        assert!(argv.iter().any(|a| a.contains("--minimized")));
        assert!(win_disable_argv().contains(&"delete".to_string()));
    }

    #[test]
    fn mac_plist_is_well_formed_xml_shape() {
        let plist = mac_plist_contents(&mac_label(), "/Applications/Veil.app/Contents/MacOS/aether-gui");
        assert!(plist.contains("<true/>"));
        assert!(plist.contains("--minimized"));
        assert!(plist.contains(&mac_label()));
        let home = PathBuf::from("/Users/test");
        assert_eq!(
            mac_plist_path(&home),
            PathBuf::from("/Users/test/Library/LaunchAgents/dev.veil.app.plist")
        );
    }
}
