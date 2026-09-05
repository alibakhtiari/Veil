# Icons (generated)

Application icons for Veil desktop and mobile applications, generated via:

```sh
npx @tauri-apps/cli@latest icon /tmp/aether-icon-square.png \
  --output aether-gui/src-tauri/icons/
```

`tauri.conf.json` references `32x32.png`, `128x128.png`,
`128x128@2x.png`, `icon.icns`, `icon.ico` — all present and required
at COMPILE time (`generate_context!()` reads them). The extra
`Square*.png` / `StoreLogo.png` / `android/` outputs are for
store listings and the Android app; keep them.
