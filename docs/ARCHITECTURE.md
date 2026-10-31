# Architecture

## Overview

```
Svelte frontend (src/)  ── invoke/listen ──▶  Tauri app (src-tauri/)  ──▶  yattt-core (crates/)
   Views, Store, Mock                     Commands, Tray, Login Webview,       Jira/Tempo Client, SQLite,
                                          Scheduler, Shutdown Hooks            Timer, Sync, Stats, Export, Secrets
```

All business logic lives in `yattt-core` and has no dependency on Tauri. The crate is testable with
`cargo test` (unit tests, HTTP tests against `wiremock`). The Tauri layer is thin: it holds the
state (`AppState`), translates commands, starts background tasks and talks to the operating
system.

## yattt-core

| Module | Purpose |
|---|---|
| `models` | Data model (Project, Sprint, Issue, TimeEntry, Credential, …), serde camelCase |
| `settings` | `Settings` with defaults, JSON file, `AppPaths` (config/data directory) |
| `secrets` | `EncryptedSecretStore`: master key in the OS keychain, payload XChaCha20-Poly1305 in `secrets.enc` |
| `jira` | `JiraClient` (Cloud/DC), `jira::parse` (JSON → model, sprint field, ADF), `jira::jql` (query building) |
| `tempo` | `TempoDataCenter` (via the Jira session) and `TempoCloud` (token) |
| `storage` | SQLite (`rusqlite`, WAL), tables `projects`, `sprints`, `issues`, `time_entries`, `meta`; backup via `VACUUM INTO` |
| `timer` | `TimerEngine`: start/stop/switch, discard short entries, idle and restore decisions |
| `sync` | Booking (create/update/delete) depending on `TempoMode`, processing of pending entries, import of existing worklogs |
| `rounding` | Rounding rules for booking |
| `stats` | Daily statistics with local day boundaries, breaks derived from gaps, target from working hours |
| `export` | CSV/JSON, summary per day and ticket |
| `idle` | Seconds since the last input (platform-specific) |
| `i18n` | Process-wide language (`Lang`, `set_lang`) and the `tr!("Deutsch", "English")` macro for texts the core produces |

### Time model

- All timestamps are stored as RFC 3339 in UTC; day boundaries, statistics and display use the
  local time zone. Entries that span midnight are split for the statistics.
- A running timer is a `time_entry` without `ended_at`. This lets it survive crashes and
  restarts. `meta.last_heartbeat` is written every 15 s.
- Jira receives `started` in the format `yyyy-MM-dd'T'HH:mm:ss.SSSZ` with the local offset; minimum duration 60 s.

### Booking states

`local` → `pending` → `synced`; a change to a booked entry → `modified` (pushed via PUT);
error → `failed` with the error text. Deleted entries with a remote ID are only removed for good
after a successful remote DELETE.

## Tauri layer

| File | Purpose |
|---|---|
| `lib.rs` | App setup, plugins, logging (file in the data directory), keyboard shortcuts, exit handling |
| `state.rs` | `AppState`: store, settings, client, user, refresh status, idle tracker, restore prompt |
| `commands.rs` | All `#[tauri::command]`s |
| `jobs.rs` | Restore session, install client (incl. cookie rotation → storage), load tickets, book |
| `login.rs` | Browser login window, cookie polling, logout incl. clearing the webview data |
| `scheduler.rs` | Tick (heartbeat, idle/sleep, tray title, reminders), auto-refresh, background sync, backups |
| `tray.rs` | Tray menu (running timers, stop, favorites as check items to start/stop, recently used tickets, Refresh, Quit), title with elapsed time |
| `shutdown/` | macOS `applicationShouldTerminate:` (NSAlert), Windows `WM_QUERYENDSESSION` (block reason + app dialog) |

### Events (backend → frontend)

`issues-updated`, `timer-changed`, `entries-changed`, `auth-changed`, `refresh-status`,
`idle-prompt`, `exit-requested`, `settings-changed`, `sync-done`, `login-window`.

## Frontend

- Svelte 5 (runes), no router: `store.view` switches between Tickets, Times, Statistics, Settings.
- `lib/api.ts`: typed wrappers around `invoke`; without Tauri (`npm run dev` in the browser),
  `lib/mock.ts` is used automatically, an in-memory backend with sample data
  (`window.__yatttMock.__simulateIdle()` and `__simulateRestore()` trigger the dialogs).
- `lib/store.svelte.ts`: global state, event subscriptions, toasts, one-second tick for running timers.
- `lib/i18n.svelte.ts`: reactive UI language (`t()`), dictionaries in `lib/locales/de.ts` and `en.ts`;
  the store applies `behavior.language` whenever settings change.
- Charts are hand-written SVGs (bars, donut, heatmap); the timesheet is a print view.

## Decisions

See [PLAN.md](PLAN.md), section 9, for the assessment of the review findings and the decisions
derived from them (cookie rotation, encrypted secrets, Tempo Cloud path, shutdown hooks,
booking per entry).
