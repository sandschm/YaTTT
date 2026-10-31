# YaTTT – Plan and Architecture

YaTTT ("Yet another Time Tracking Tool") is a desktop app for tracking time on Jira tickets,
with booking to Jira/Tempo and a local backup copy. The feature set is modeled on Tyme
(see [TYME_ANALYSIS.md](TYME_ANALYSIS.md)). Target platforms: macOS, Windows, Linux.

## 1. Requirements (from the task description)

| # | Requirement | Solution |
|---|---|---|
| A1 | Time tracking on Jira tickets, Tempo is in use | Worklogs via Jira REST (Tempo syncs automatically) or via the Tempo API |
| A2 | Additional local storage | SQLite in the user data directory, daily backup |
| A3 | Feature set like Tyme | See the feature matrix in TYME_ANALYSIS.md |
| A4 | All tasks and projects come from Jira | No local creation of projects/tasks; tickets are cached |
| A5 | Settings: Jira URL | Settings page "Connection" |
| A6 | Ask for login/password at login, store them securely locally | Login in an embedded browser window (SSO/2FA) or via a form; session/token in the OS keychain (`keyring`), never in plain text in files |
| A7 | Selection of projects and sprints, with search | Settings "Projects & Sprints" with search fields |
| A8 | Filter out ticket types (e.g. epics) | Setting "Hidden ticket types" |
| A9 | Always load the tickets of the current sprint of all selected projects | JQL `project in (…) AND sprint in openSprints()` in addition to the selected sprints |
| A10 | Regular refresh, configurable interval, can be triggered manually | Scheduler in the Rust backend, refresh button |
| A11 | Shutdown/restart while a timer is running: ask whether to stop or keep running | Native hooks (macOS `applicationShouldTerminate:`, Windows `WM_QUERYENDSESSION`), plus a restore prompt on the next start on all platforms |
| A12 | Rust, macOS/Windows/Linux | Tauri 2 (Rust backend) + Rust core crate; UI in Svelte/TypeScript |
| A13 | Release pipeline on GitHub | GitHub Actions: CI (test/lint) and release (tauri-action, 3 platforms) |
| A14 | API access without an API token, with SSO and 2FA | Browser session login in a webview, cookies are taken over; see section 4 |

## 2. Technology decision

### Why Tauri 2 and not a pure Rust GUI (egui/iced/Slint)?

Requirement A14 (SSO + second factor, no API token) can only be met if the user can go through
the real Jira login dialog (including the redirect to the identity provider and 2FA).
For that, the app needs an embedded browser from which it can then take over the session cookies.
Tauri 2 offers exactly that: `Webview::cookies_for_url()` also returns HttpOnly cookies
on all three platforms (WKWebView, WebView2, WebKitGTK). Pure Rust GUIs have no
embedded browser; a login there would only work with an API token/PAT.

Consequence: all business logic (Jira client, Tempo, timer, storage, export, idle, scheduler,
shutdown hooks) lives in Rust. The UI is HTML/Svelte/TypeScript and talks to the backend
exclusively through Tauri commands and events. The Rust logic lives in its own crate
(`yattt-core`) without a Tauri dependency and is therefore testable with `cargo test`.

Alternative, deliberately not chosen: Leptos/Yew as a Rust→WASM frontend. Advantage: 100% Rust.
Disadvantage: considerably slower development and toolchain complexity (Trunk, wasm32) for a
UI with many views. It can be replaced later, since the interface (commands/events) is stable.

### Stack

| Area | Choice |
|---|---|
| App shell | Tauri 2 (tray icon, menus, webview login) |
| Backend logic | Rust, crate `yattt-core` |
| HTTP | `reqwest` (rustls), cookie header set manually from the stored session |
| Storage | `rusqlite` (bundled), migrations in code |
| Settings | JSON in the configuration directory (`dirs`) |
| Secrets | `keyring` (macOS Keychain, Windows Credential Manager, Linux Secret Service) |
| Idle detection | `user-idle` (CGEventSource / GetLastInputInfo / X11), Wayland: time-jump watchdog |
| Scheduler | `tokio` tasks in the Tauri runtime |
| Frontend | Svelte 5, TypeScript, Vite; charts as custom SVG components |
| Tauri plugins | notification, dialog, opener, autostart, global-shortcut, single-instance, store (window state) |
| Tests | `cargo test` (unit), `wiremock` (Jira client against a mock server), `vitest` (frontend helpers) |
| CI/CD | GitHub Actions, `tauri-apps/tauri-action` |

## 3. Architecture

```
┌──────────────────────────── Tauri app (src-tauri) ───────────────────────────────┐
│  commands.rs  – thin Tauri commands, mapped onto yattt-core                      │
│  events       – timer-tick, issues-updated, sync-status, auth-expired, …         │
│  tray.rs      – tray icon/menu, title with the running time                      │
│  login.rs     – open login webview, poll cookies, verify session                 │
│  shutdown/    – macOS applicationShouldTerminate:, Windows WM_QUERYENDSESSION    │
│  scheduler.rs – refresh interval, idle watchdog, reminders, sync retry           │
└────────────────────────────────────┬─────────────────────────────────────────────┘
                                     │ Rust API
┌────────────────────────────── yattt-core ────────────────────────────────────────┐
│  settings   – Settings struct, load/save, defaults, validation                   │
│  auth       – AuthMethod (session cookies, Basic, PAT, ApiToken), SecretStore    │
│  jira       – JiraClient: serverInfo, myself, projects, boards, sprints, search, │
│               issue types, worklog create/update/delete; Cloud/DC differences    │
│  tempo      – TempoClient (DC: /rest/tempo-timesheets/4, Cloud: api.tempo.io/4)  │
│  storage    – SQLite: projects, issues, time_entries, sync_queue, meta; backups  │
│  timer      – TimerEngine: start/stop/switch, idle handling, rounding            │
│  sync       – worklog booking, retry, import of existing worklogs                │
│  export     – CSV/JSON, timesheet data (days, breaks, target/actual)             │
│  stats      – aggregations for the Statistics view                               │
│  idle       – platform abstraction for idle time                                 │
└──────────────────────────────────────────────────────────────────────────────────┘
                                     ▲ Tauri invoke/listen
┌────────────────────────── Frontend (src, Svelte) ────────────────────────────────┐
│  Views:   timer bar · Tickets · Times (list/week) · Statistics ·                 │
│           Settings (Connection, Projects & Sprints, Filter, Time, Behavior)      │
│  Dialogs: login, idle return, quit with running timer, restore                   │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Data model (SQLite)

- `projects(key PK, id, name, avatar_url, color, selected, billable_default, sort)`
- `sprints(id PK, board_id, name, state, start_date, end_date, project_key, selected)`
- `issues(key PK, id, project_key, summary, issue_type, issue_type_id, status, status_category, parent_key, epic_key, sprint_id, assignee, due_date, original_estimate_s, remaining_estimate_s, time_spent_s, updated, url, last_seen, favorite)`
- `time_entries(id PK, issue_key, started_at, ended_at NULL, note, billable, rounded_seconds NULL, sync_state, jira_worklog_id, tempo_worklog_id, sync_error, created_at, updated_at, deleted_at)`
- `sync_queue(id, entry_id, action, attempts, next_attempt_at, last_error)`
- `meta(key, value)` – schema version, last refresh, last clean exit, timers running at exit

Secrets (cookies, tokens, the password for Basic auth) are stored exclusively in the keychain
under the service `de.yattt.app`, account = Jira URL.

### Timer semantics

- A time entry with `ended_at = NULL` is a running timer. This lets the timer survive app crashes,
  restarts and shutdown: on the next start, the app finds open entries and asks what to do
  ("keep running until now" / "stop at the time of quitting" / "stop at time X").
- `meta.last_heartbeat` is written every 15 s. On start, the time of the last sign of life
  is therefore known in case the machine went off without asking.
- Single-timer mode: starting a timer stops the running one. Multiple timers are optional.
- Idle: once idle time exceeds the threshold, the backend remembers when idling began; on return, a dialog
  with three options is shown. Sleep is detected when the tick interval clearly exceeds the expected cadence.
- Rounding is applied only when booking; local data stays accurate to the second.

### Shutdown/restart/quit

| Platform | Mechanism | Behavior |
|---|---|---|
| macOS | Custom implementation of `applicationShouldTerminate:` on the tao app delegate (added at runtime via `class_addMethod`), since tao does not implement the method | With a running timer, a native `NSAlert` ("Stop timer" / "Keep running"), then `NSTerminateNow`; without a timer, quit immediately |
| Windows | Subclassing the main window, `WM_QUERYENDSESSION` + `ShutdownBlockReasonCreate` | `MessageBox` with the same options, then return `TRUE` |
| Linux | No reliable dialog during shutdown (logind delay lock max. 5 s) | State is secured by the heartbeat; prompt on the next start |
| All | `RunEvent::ExitRequested` (Cmd+Q, tray "Quit") | In-app dialog "Stop timer and quit" / "Keep running and quit" / "Cancel" |
| All | Closing the window | Minimizes to the tray (setting) |

"Keep running" means: the entry stays open and keeps counting; on the next start,
the timer visibly continues to run, with a notice.

## 4. Jira access without an API token, with SSO and 2FA

### Finding

- **Jira Cloud:** Basic auth with a password has been disabled since 2019; what remains is API tokens, OAuth 2.0 (3LO)
  or the browser session (`cloud.session.token`, HttpOnly). Login always goes through
  `id.atlassian.com`/`auth.atlassian.com`; SSO (SAML via Atlassian Guard) and 2FA happen there.
  Test: `https://jira.atlassian.com/login.jsp` redirects to `auth.atlassian.com/authorize?...`.
  Atlassian calls cookie auth for external clients "deprecated", but the browser itself uses
  exactly this session for all REST calls. An app that embeds the browser login behaves like the browser.
- **Jira Data Center/Server:** A session via `POST /rest/auth/1/session` (username/password) returns a `JSESSIONID`.
  With SAML/Crowd SSO and 2FA plugins, this route often does not work (the login is redirected to the IdP).
  After successful SSO/2FA, the browser login sets `JSESSIONID` (and possibly `seraph.rememberme.cookie`),
  which are valid for REST calls. PATs (from 8.14) are a clean alternative, but they are tokens.
- Tested against public instances (anonymously): ASF Jira (DC 10.3) returns `serverInfo`, `project`, `search`;
  `myself` and `agile/board` require authentication (401). Jira Cloud (`jira.atlassian.com`) requires a
  login for REST without a session (302). The API shapes for Cloud (`/rest/api/3/search/jql`, `nextPageToken`)
  and DC (`/rest/api/2/search`, `startAt`) differ and are abstracted in the client.

### Solution: browser session login (default)

1. The user enters the Jira URL and clicks "Sign in".
2. The app opens a Tauri webview window at `<jira-url>/login.jsp?os_destination=/rest/api/2/myself`
   (Cloud: redirect to Atlassian ID, then the IdP, then 2FA, back to the site).
3. Every 1.5 s, the backend reads `cookies_for_url(jira-url)` and calls
   `GET /rest/api/2/myself` with these cookies. Response 200 → login successful.
4. The cookies (name, value, domain, path, expiry) are stored as a session in the keychain, and the window closes.
5. All REST calls send `Cookie: …` and `X-Atlassian-Token: no-check` (XSRF exemption for
   non-form requests). On 401/403, `auth-expired` is reported and "Sign in again" is offered;
   the session cookies in the webview profile are kept, so signing in again often succeeds without
   entering the password again.

Fallback options in the settings: username/password (DC, `/rest/auth/1/session` or Basic),
Personal Access Token (DC), email + API token (Cloud). The password is stored only in the keychain.

### Test strategy for auth

- Unit/integration tests of the client against `wiremock` (session cookie, 401 handling, Cloud/DC pagination).
- Manual test of the webview cookie takeover against `issues.apache.org/jira` (sets `JSESSIONID` even anonymously).
- The real SSO/2FA test is only possible against the team's Jira instance (checklist in [JIRA_AUTH.md](JIRA_AUTH.md)).

## 5. Tempo

- **Default:** worklog via `POST /rest/api/2/issue/{key}/worklog` (`started`, `timeSpentSeconds`, `comment`).
  Tempo Cloud picks up Jira worklogs via webhook; Tempo DC stores worklogs in Jira anyway.
- **Tempo DC directly (optional):** `POST /rest/tempo-timesheets/4/worklogs` with the same Jira session
  (`originTaskId`, `started`, `timeSpentSeconds`, `billableSeconds`, `worker`, `comment`).
- **Tempo Cloud directly (optional):** `POST https://api.tempo.io/4/worklogs` with a Tempo token
  (`issueId`, `authorAccountId`, `startDate`, `startTime`, `timeSpentSeconds`, `billableSeconds`).
- Each entry remembers its `jira_worklog_id`/`tempo_worklog_id`; changes to the entry lead to a
  `PUT`, deletion to a `DELETE`. Failures end up in the `sync_queue` with backoff.

## 6. User interface

- **Timer bar:** current timer (ticket, time, note), stop, daily total, quick-start search.
- **Tickets:** groups per project (color), sprint badge, type icon, status, target/actual bar,
  start button, favorites (max. 15, with a notice dialog beyond that), filter bar (sprint, type, component, label, status, only mine), search, refresh with timestamp.
- **Tray:** favorites as check items; a click starts or stops the timer, the check mark shows the running one.
- **Filter per project:** `selection.projectFilters` (project → components, labels, and/or) extends the JQL with
  `(project not in (…) OR (project = X AND (component in (…) AND|OR labels in (…))))`, so that only the
  affected project is filtered. Components come from `/rest/api/2/project/{key}/components`.
- **Times:** daily list with entries and daily totals, edit/delete, booking status,
  "Book now"; weekly timeline with blocks; breaks as gaps.
- **Statistics:** period selection; key figures (target/actual, overtime/undertime, breaks, billable share);
  bars per day; distribution per project/ticket; heatmap.
- **Settings:** Connection (URL, login method, status, sign out) · Projects & Sprints (search,
  selection) · Filter (ticket types, hide completed) · Time (rounding, working hours per weekday,
  week start, time format, short entries) · Behavior (interval, idle threshold, multiple timers,
  autostart, minimize to tray, notifications, note on stop, book on stop) ·
  Tempo (mode, token) · Export.

## 7. Milestones

1. **M1 Foundation:** workspace, `yattt-core` with settings/storage/models, Tauri shell, window, tray.
2. **M2 Jira:** client (Cloud/DC), login webview, keychain, loading projects/sprints/issues, scheduler.
3. **M3 Time tracking:** TimerEngine, entries, idle/sleep, Times view, timer bar, hotkeys.
4. **M4 Booking:** worklogs to Jira/Tempo, queue, status display, import of existing worklogs.
5. **M5 Convenience:** statistics, export, reminders, shutdown hooks, restore.
6. **M6 Delivery:** tests, CI, release pipeline, documentation.

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Cookie login is blocked by Atlassian or the instance (captcha, WAF) | Fallback to PAT/API token; show errors clearly |
| Session expires (Cloud: a few days) | 401 detection, "Sign in again" banner, webview keeps the browser session |
| XSRF check on POST with cookie auth | `X-Atlassian-Token: no-check`, JSON content type, no `Origin` header |
| `Webview::cookies()` can block in synchronous commands on Windows | Call `cookies_for_url` only from async tasks |
| Tray/hotkeys limited under Wayland | Document it; the app remains usable without a tray |
| Idle detection not available under Wayland | Time-jump watchdog stays active, idle detection can be turned off |
| Shutdown dialog not possible on Linux | Heartbeat + prompt on the next start |
| Cloud search uses the new pagination | Client detects `deploymentType` and picks the endpoint |
| No code signing | Unsigned release at first; secrets for signing prepared and documented |

## 9. Review results and decisions

The plan was reviewed independently, see [PLAN_REVIEW.md](PLAN_REVIEW.md) (15 findings). Decisions:

| Finding | Decision |
|---|---|
| 1 Cookie login as the only path | Browser login remains the default (requirement: no API token, SSO, 2FA). Added: cookie rotation is applied (every `Set-Cookie` response updates the stored session), login webview with a desktop browser user agent (IdPs otherwise block embedded webviews). OAuth 2.0 (3LO, system browser + localhost callback) is planned as a second official path (credential variant in the model, implementation after v0.1, since the client has to register the app). A spike against the real instance is the first step after handover, see [JIRA_AUTH.md](JIRA_AUTH.md). |
| 2 Tauri instead of a pure Rust GUI | Kept. Honest rationale: productivity, platform integration, webview login. Business logic entirely in `yattt-core`. Must be confirmed by the client. |
| 3 Tempo Cloud syncs Jira worklogs only overnight | With Jira Cloud, the Tempo Cloud API path (Tempo token) is recommended as the default and suggested in the UI; Jira worklog remains the default for Data Center. |
| 4 Shutdown hooks | macOS: `applicationShouldTerminate:` via `class_replaceMethod` (adds or replaces, robust against future tao versions), synchronous `NSAlert`. Windows: `WM_QUERYENDSESSION` → `ShutdownBlockReasonCreate` + `FALSE`, decision in the app dialog, `WM_ENDSESSION` saves the state. Linux: heartbeat and prompt on start (session inhibit later). The heartbeat remains the guarantee on all platforms. |
| 5 Session does not fit in the Windows Credential Manager | Adopted: the keychain holds only a random 32-byte key; session, tokens and passwords are stored XChaCha20-Poly1305-encrypted in `secrets.enc` in the configuration directory. Without a keychain (Linux without Secret Service), the key is stored in a file with permissions 0600, with a warning in the settings. |
| 6 Booking per entry vs. cluster | v0.1 books per entry (traceable, simple correction via PUT/DELETE). Rounding is off by default; the settings note that rounding applies per entry. Cluster booking (one worklog per day and ticket) is planned as follow-up work. Timestamps: UTC in SQLite, local time for day boundaries, Jira `started` with offset. A minimum duration of 60 s is enforced. Multiple timers are off by default. Sprints belong to boards, not to projects. |
| 7 Endpoints | Cloud `/rest/api/2/search/jql` with `nextPageToken`, DC `/rest/api/2/search` with `startAt`; fields always explicit; 429 with `Retry-After` is retried once. |
| 8 Internal CAs/proxy | `reqwest` with `rustls-tls-native-roots` (system certificate store), system proxy via environment variables. |
| 9 Idle under Wayland | `user-idle` stays (X11/macOS/Windows); Wayland via time-jump watchdog. Switching to `user-idle3` was evaluated, see risks. |
| 10 Signing | The pipeline supports signing/notarization via secrets (`APPLE_CERTIFICATE`, `APPLE_ID`, `TAURI_SIGNING_PRIVATE_KEY`); without secrets, unsigned artifacts are built. The client has to provide the certificates. |
| 11 Tauri details | `tauri-plugin-window-state`, feature `tray-icon`, "Sign out" clears the webview data (`clear_all_browsing_data`). |
| 12 Tyme matrix | Multiple timers with a warning, duration directly editable, duplicate/move entry in the Times view. UI language: German and English, switchable in the settings (since 0.4.0). |
| 13 A6 | Deviation documented: with SSO, the session is stored, never the password. Only with username/password against Data Center without SSO is the password stored (encrypted) locally. |
| 14 Order | CI from M1 on; heartbeat/restore in M3; shutdown hooks in M4 instead of M5; import of existing worklogs after v0.1. |
| 15 Minor points | WAL is active, backup via the SQLite backup API, log file in the data directory. |

## 10. Implementation status (31.10.2026)

| Area | Status |
|---|---|
| `yattt-core` | Implemented: settings, encrypted secrets, Jira client (Cloud/DC, cookie rotation, 429 retry), JQL building, Tempo DC/Cloud, SQLite, TimerEngine, booking/sync/import, statistics, export, rounding, idle. 47 tests green, Clippy without warnings. |
| Tauri app | Implemented: commands, login webview with cookie takeover, tray (menu, title), scheduler (heartbeat, idle/sleep, auto-refresh, background sync, reminders, backups), global shortcuts, autostart, shutdown hooks for macOS/Windows, restore prompt. |
| Frontend | Implemented: timer bar, Tickets (groups, filters including labels, search, favorites with limit, target/actual), Times (list, week, editor, booking, undo), Statistics (KPIs, bars, donut, top tickets, heatmap, export, timesheet), Settings (7 sections), dialogs (idle, restore, quit, note). Mock backend for development in the browser. Since 0.4.0 the UI, tray menu, notifications and error messages are available in German and English (setting "Language": system, German, English). |
| CI/Release | CI is green on GitHub (ubuntu-22.04, macOS, Windows). The release workflow builds Linux (.deb/.rpm/.AppImage), Windows (.msi/-setup.exe) and macOS (universal .dmg); signing only when secrets are set. First complete release: v0.1.1; v0.2.0 with tray favorites and label filter (31.10.2026). |
| Tested | Core tests; UI in the browser (mock); native app on macOS: shutdown hook installed, backup written, SSO/2FA login against the team's Jira Cloud instance confirmed (31.10.2026); tray favorites and label filter confirmed in the native app. |
| Open | Booking against the team's Jira instance not yet verified in real use; Windows/Linux builds verified only via CI; Windows shutdown hook untested; OAuth 2.0, cluster booking and Linux session inhibit as follow-up work. |
