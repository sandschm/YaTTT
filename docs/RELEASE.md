# Release process

## Pipelines

- **CI** (`.github/workflows/ci.yml`): on every push/PR, runs `cargo fmt --check`,
  `cargo test -p yattt-core`, `cargo clippy`, `npm run check`, `npm run build`, `cargo check -p yattt` on three platforms.
- **Release** (`.github/workflows/release.yml`): on a `v*` tag (or manually),
  `tauri-apps/tauri-action` builds the installer packages and creates a **draft release**:
  - macOS: universal `.dmg` (Apple Silicon + Intel)
  - Windows: `.msi` (WiX) and `-setup.exe` (NSIS, per-user installation)
  - Linux: `.deb`, `.rpm`, `.AppImage`

## Bumping the version and publishing

1. Set the version in `package.json` (the Tauri configuration reads it from there) and match it
   in `Cargo.toml` (`[workspace.package] version`).
2. Commit the changes, create the tag and push:

```bash
git tag v0.4.0 && git push origin main --tags
```

3. Add an entry to `CHANGELOG.md`.
4. After the run, review the draft release under "Releases" and publish it.

## Signing (optional, recommended)

Without a signature, macOS Gatekeeper blocks the first launch (right-click → Open, or
`xattr -dr com.apple.quarantine`) and Windows SmartScreen shows a warning. For signed builds,
set the following repository secrets:

| Secret | Meaning |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application certificate as Base64 (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password of the `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Company (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | For notarization (app-specific password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Only needed if the updater plugin is enabled |

Windows code signing can be added via `bundle.windows.certificateThumbprint` in
`src-tauri/tauri.conf.json` and a certificate installed on the runner.

Note: The workflow only exports the signing variables if the secrets are set. Otherwise an
empty `APPLE_CERTIFICATE` value makes the macOS build fail during signing (as happened in the
first release run, v0.1.0).

## Local build

```bash
npm run tauri build
```

The packages are then located under `target/release/bundle/`.
