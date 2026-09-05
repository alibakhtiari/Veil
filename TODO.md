# Veil — Remaining Tasks & Release Roadmap

This document tracks all strictly **pending tasks**, **release recipes**, and **architectural milestones** for Veil. Completed features are omitted.

---

## 1. Desktop GUI (`aether-gui`)

### 1.1 Auto-Updater Activation
- [x] Generate Minisign key pair:
  ```sh
  npm --prefix aether-gui/frontend run tauri signer generate
  ```
- [x] Add the generated public key to `"pubkey"` in `aether-gui/src-tauri/tauri.conf.json` and toggle `"active": true` before tagging production v1.0.0.
- [ ] Configure `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) in GitHub repository secrets.

### 1.2 Desktop Elevation UX
- [x] Verify and polish non-root elevation flow (`pkexec` on Linux, `osascript` on macOS) when virtual network adapters and routing tables are bound.

---

## 2. Native Android App (`android/`)

### 2.1 UI Polish
- [x] Jetpack Compose polish: real-time upload/download speed metrics and bandwidth counters.
- [x] Quick Settings tile and Always-On VPN handler.
- [x] Battery optimization exemption prompt.

### 2.2 Release Keystore Secrets
Configure in GitHub repository secrets for the `gui-android` CI release job:
- [ ] `ANDROID_KEYSTORE_BASE64`
- [ ] `ANDROID_KEYSTORE_PASSWORD`
- [ ] `ANDROID_KEY_ALIAS`
- [ ] `ANDROID_KEY_PASSWORD`

### 2.3 On-Device Smoke Test
1. Download `app-release.apk` from GitHub Actions artifacts or Release page.
2. Install via ADB:
   ```sh
   adb install -r app-release.apk
   ```
3. Verify:
   - App requests VPN permission on first Connect.
   - `VpnTunnelService` activates and status bar key icon appears.
   - Traffic flows through the tunnel (`warp=on`).

---

## 3. Release CI Secrets

Configure in GitHub repository secrets for automated tagged releases in `.github/workflows/release.yml`:
- [ ] `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- [ ] `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` (macOS Developer ID code signing & notarization).
