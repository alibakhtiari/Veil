# Veil — Remaining Tasks & Release Roadmap

This document tracks all strictly **pending tasks**, **release secrets**, and **on-device verification recipes** for Veil. Completed features and milestones are omitted.

---

## 1. Release CI Secrets

Configure in GitHub repository secrets (`Settings` → `Secrets and variables` → `Actions`) for automated release builds in `.github/workflows/release.yml`:

### 1.1 Desktop Auto-Updater Signing
- [ ] `TAURI_SIGNING_PRIVATE_KEY` — Private Minisign key corresponding to the public key configured in `tauri.conf.json`.
- [ ] `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — Passphrase used when generating the Minisign key pair (empty string if no passphrase).

### 1.2 Android APK/AAB Release Signing
Generate a release keystore:
```sh
keytool -genkeypair -v -keystore release.keystore -alias veil -keyalg RSA -keysize 2048 -validity 10000
base64 -i release.keystore | tr -d '\n'
```
Configure:
- [ ] `ANDROID_KEYSTORE_BASE64` — Base64-encoded content of `release.keystore`.
- [ ] `ANDROID_KEYSTORE_PASSWORD` — Password chosen for the keystore.
- [ ] `ANDROID_KEY_ALIAS` — `veil` (or custom key alias).
- [ ] `ANDROID_KEY_PASSWORD` — Password for the key alias.

### 1.3 macOS Developer ID Code Signing & Notarization (Optional)
For gatekeeper-notarized macOS `.dmg` builds:
- [ ] `APPLE_CERTIFICATE` — Base64-encoded Developer ID Application `.p12` certificate.
- [ ] `APPLE_CERTIFICATE_PASSWORD` — Certificate export password.
- [ ] `APPLE_SIGNING_IDENTITY` — Developer ID identity string.
- [ ] `APPLE_ID` — Apple account email.
- [ ] `APPLE_PASSWORD` — App-specific password generated from appleid.apple.com.
- [ ] `APPLE_TEAM_ID` — 10-character Apple Developer Team ID.

---

## 2. On-Device Smoke Test (Android)

1. Download `app-release.apk` from the GitHub Actions release artifact or Release page.
2. Install via ADB:
   ```sh
   adb install -r app-release.apk
   ```
3. Verify on device:
   - [ ] App launches into Material 3 dashboard and checks VPN consent.
   - [ ] Tap **Connect**: platform consent dialog appears; approving establishes `VpnTunnelService`.
   - [ ] Real-time bandwidth metrics (download/upload speed and total transfer) update dynamically.
   - [ ] Battery optimization exemption prompt requests exclusion from Android battery restrictions.
   - [ ] Quick Settings tile toggles connection state and updates status.
   - [ ] Verify outbound connectivity (`warp=on` over Cloudflare edge).
