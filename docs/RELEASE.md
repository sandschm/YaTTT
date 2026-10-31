# Release-Prozess

## Pipelines

- **CI** (`.github/workflows/ci.yml`): bei jedem Push/PR auf drei Plattformen `cargo fmt --check`,
  `cargo test -p yattt-core`, `cargo clippy`, `npm run check`, `npm run build`, `cargo check -p yattt`.
- **Release** (`.github/workflows/release.yml`): bei einem Tag `v*` (oder manuell) baut
  `tauri-apps/tauri-action` die Installationspakete und legt einen **Release-Entwurf** an:
  - macOS: universelles `.dmg` (Apple Silicon + Intel)
  - Windows: `.msi` (WiX) und `-setup.exe` (NSIS, Installation pro Benutzer)
  - Linux: `.deb`, `.rpm`, `.AppImage`

## Version erhöhen und veröffentlichen

1. Version in `package.json` setzen (die Tauri-Konfiguration liest sie von dort) und in
   `Cargo.toml` (`[workspace.package] version`) angleichen.
2. Änderungen committen, Tag setzen und pushen:

```bash
git tag v0.1.1 && git push origin main --tags
```

3. Eintrag in `CHANGELOG.md` ergänzen.
4. Nach dem Lauf den Release-Entwurf unter „Releases“ prüfen und veröffentlichen.

## Signierung (optional, empfohlen)

Ohne Signatur blockiert macOS Gatekeeper den ersten Start (Rechtsklick → Öffnen bzw.
`xattr -dr com.apple.quarantine`), Windows SmartScreen warnt. Für signierte Builds folgende
Repository-Secrets setzen:

| Secret | Bedeutung |
|---|---|
| `APPLE_CERTIFICATE` | Developer-ID-Application-Zertifikat als Base64 (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | Passwort der `.p12` |
| `APPLE_SIGNING_IDENTITY` | z. B. `Developer ID Application: Firma (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Für die Notarisierung (App-spezifisches Passwort) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Nur nötig, wenn das Updater-Plugin aktiviert wird |

Windows-Code-Signing lässt sich über `bundle.windows.certificateThumbprint` in
`src-tauri/tauri.conf.json` und ein im Runner installiertes Zertifikat ergänzen.

Hinweis: Die Signatur-Variablen werden im Workflow nur exportiert, wenn die Secrets gesetzt sind.
Ein leerer `APPLE_CERTIFICATE`-Wert lässt den macOS-Build sonst beim Signieren scheitern (so
geschehen beim ersten Release-Lauf v0.1.0).

## Lokaler Build

```bash
npm run tauri build
```

Die Pakete liegen danach unter `target/release/bundle/`.
