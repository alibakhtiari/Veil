# Veil — Remaining Tasks & Release Roadmap

This document tracks strictly **pending repository secrets configuration** and **on-device verification recipes** for Veil.

---

## 1. Release CI Secrets

Add these pre-generated values to GitHub repository secrets (`Settings` → `Secrets and variables` → `Actions` → `New repository secret`):

### 1.1 Desktop Auto-Updater (Tauri Minisign)

The public key is configured in `aether-gui/src-tauri/tauri.conf.json`. Configure the matching signing secret:

- **`TAURI_SIGNING_PRIVATE_KEY`**:
  ```text
  dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5L1NiUVZFOVI3MzBLQjEya09XYVdxSDJtTHdQUSt2VEsxU3Z2MXpHRVpGa0FBQkFBQUFBQUFBQUFBQUlBQUFBQXh2b0ttdDJCZ1grdlh2aWQxd292b3FiekZ3dGQ1K1VnN2JSS01OSWZyYzYwUlpzakt4MGdRcmo2bm9jSmd4S0tJZ24yL29NVGpjL0hIRlR5TEtPaVpnZDFIL2lpSUFOQ3RBb3N3dUQxakc1QXdIdmZxcUtuY1FZdC9KUEFYbXFWelBTWDEweVJIeHM9Cg==
  ```
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**:
  Leave empty / unset (key was generated without a passphrase).

---

### 1.2 Android APK & AAB Release Keystore

The 2048-bit RSA release keystore has been generated with validity through 2054 (10,000 days):

- **`ANDROID_KEYSTORE_BASE64`**:
  ```text
  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
  ```
- **`ANDROID_KEYSTORE_PASSWORD`**: `veil_release_2026`
- **`ANDROID_KEY_ALIAS`**: `veil`
- **`ANDROID_KEY_PASSWORD`**: `veil_release_2026`

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
