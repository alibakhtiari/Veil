# Veil — Release Guide & Verification Checklist

All code tasks and GitHub Actions release secrets (`TAURI_SIGNING_PRIVATE_KEY`, `ANDROID_*`) have been configured.

---

## 1. How to Publish a Release on GitHub

You can publish a release in one of two ways:

### Method A: Git Tag (Recommended)
Run from the repository terminal:
```bash
git tag v1.0.0
git push origin v1.0.0
```

### Method B: GitHub CLI
```bash
gh release create v1.0.0 --title "Veil v1.0.0" --generate-notes
```

### Method C: GitHub Web UI
1. Navigate to: https://github.com/alibakhtiari/Veil/releases/new
2. Click **Choose a tag**, enter `v1.0.0`, and select **Create new tag: v1.0.0 on publish**.
3. Enter release title `Veil v1.0.0`.
4. Click **Publish release**.

---

### What GitHub Actions Builds & Publishes Automatically:
- **Windows:** `.msi` and `.exe` installers.
- **macOS:** Universal `.dmg` bundle (Apple Silicon + Intel).
- **Linux:** `.AppImage`, `.deb`, and `.rpm` packages (x86_64 & arm64).
- **Android:** Signed `app-release.apk` and `app-release.aab`.
- **Auto-Updater:** Minisign-signed `.tar.gz` and `.zip` update bundles.
- **Checksums:** Individual `.sha256` files and a unified `SHA256SUMS.txt`.

---

## 2. On-Device Smoke Test (Android)

Once the release workflow finishes:
1. Download `app-release.apk` from the Release page or workflow artifacts.
2. Install via ADB:
   ```sh
   adb install -r app-release.apk
   ```
3. Verification Checklist:
   - [ ] App launches into Material 3 dashboard.
   - [ ] Tap **Connect**: platform VPN permission dialog appears.
   - [ ] Live bandwidth counters and download/upload speed meters update dynamically.
   - [ ] Battery optimization exemption prompt works.
   - [ ] Quick Settings tile toggles tunnel on and off.
   - [ ] Outbound traffic flows securely through Cloudflare edge (`warp=on`).
