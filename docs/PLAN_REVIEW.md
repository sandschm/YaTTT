# Review of PLAN.md and TYME_ANALYSIS.md

As of: 30.10.2026. Sources spot-checked online (docs.rs/tauri, Atlassian documentation, Tempo KB,
Microsoft documentation, tao source code).

## Overall assessment

The plan is well structured, the requirements matrix is easy to follow, and the architecture (`yattt-core`
without a Tauri dependency) and the data model are solid starting points. The Tyme analysis is
careful, and its mappings are mostly plausible.

The weaknesses lie where the plan builds on unverified assumptions: (1) cookie login as the
only serious auth path, (2) "Tempo picks up Jira worklogs automatically", (3) the
shutdown dialogs on Windows/macOS, (4) the session in the OS keychain, (5) the booking logic in the
data model. All of them are solvable, but they must be clarified before M2 because they affect the
architecture and the order of work. Tauri 2 is defensible (finding 2), but it needs to be agreed with
the client with regard to "written in Rust".

## Findings

### 1. Cookie login as the only default path is too risky — High

Finding: Cookie auth is officially deprecated for Jira Cloud. Not addressed:
- Identity providers block embedded webviews: Google Workspace rejects logins from webviews
  (`disallowed_useragent`); Microsoft Entra Conditional Access ("compliant device") often fails in
  WebView2/WKWebView because the browser SSO plugin is missing. The client's IdP is unknown.
- Atlassian rotates session cookies via `Set-Cookie`. A cookie header set "manually from the stored
  session" loses those rotations; the session expires earlier.
- Session duration can be controlled by Atlassian Guard policy (idle from 15 min., absolute e.g. 12 h).
- The referenced `JIRA_AUTH.md` does not exist.

Recommendation:
- Clarify up front: Cloud or Data Center, which IdP, which Tempo. That decides the default path.
- Plan OAuth 2.0 as an equivalent, official path, not just as a token fallback:
  Cloud: 3LO with the system browser and an `http://localhost:<port>` callback — SSO/2FA run in the real
  browser. Limitations: no PKCE in Cloud (client secret in the binary, an accepted risk for
  desktop apps), app registration in the Developer Console, org admins can block 3LO,
  calls via `api.atlassian.com/ex/jira/{cloudId}`. Data Center ≥ 8.22: OAuth 2.0 "Incoming
  Link" with PKCE, no secret; the redirect must be HTTPS in production, localhost only via a
  system property (clarify with the Jira admin).
- Clarify "without API token" with the client: it probably means "no manually created
  token". OAuth meets the spirit of this, but needs a one-time admin registration.
- Keep the cookie login, but with a cookie jar (reqwest `cookie_store`) and persistence after every
  response. Before M2, run a spike against the real instance with SSO/2FA — the biggest project risk.

### 2. Technology decision Tauri 2 + Svelte — Medium

Finding: "Only Tauri can do SSO/2FA" holds only for the webview cookie path; with OAuth in the system
browser, a pure Rust GUI would also be feasible. Tauri remains pragmatic nonetheless: mature tray,
notification and autostart plugins, HTML for timeline/heatmap/printing, webview login as an option.
According to docs.rs, `cookies_for_url` returns HttpOnly cookies on all desktop platforms; the
Windows deadlock in synchronous commands is correctly identified.

Recommendation: Keep the decision, state the rationale honestly (productivity,
platform integration) and get confirmation that "in Rust" refers to the business logic/backend. Strictly
ensure that `yattt-core` contains all of the logic.

### 3. Tempo Cloud picks up Jira worklogs only overnight — High

Finding: According to the Tempo KB, worklogs created via Jira REST are synced to Tempo "overnight";
with the Tempo provider active, Jira's "Log Work" is disabled; Tempo attributes
(account, billable) cannot be set via the Jira API. For Tempo Cloud users, the "default" path means
up to a day of delay and incomplete data. For Tempo DC the assumption is correct.

Recommendation: With Tempo Cloud, use the Tempo API as the default. Tempo Cloud offers OAuth 2.0
Authorization Code (registration under Settings → API Integration; the code expires after 60 s) —
no manual token, but admin registration. Tempo DC as planned, with the Jira session.

### 4. Shutdown hooks do not work as described — High

Finding:
- Windows: The user never sees a `MessageBox` in `WM_QUERYENDSESSION`; since Vista, Windows shows a
  full-screen blocking screen. "MessageBox, then `TRUE`" quits without a decision.
  `WM_ENDSESSION` can also arrive after `FALSE`.
- macOS: tao indeed does not implement `applicationShouldTerminate:` (verified:
  `TaoAppDelegateParent`). `class_addMethod` works today, but fails silently as soon as
  tao implements the method itself. A synchronous `NSAlert` blocks logout.
- Linux: "not possible" is too pessimistic. `org.gnome.SessionManager.Inhibit` (LOGOUT) shows
  the app with a reason in the end-session dialog; KDE is similar. Not reliable, but better than nothing.

Recommendation: Windows: `ShutdownBlockReasonCreate("Timer running …")` + `FALSE`; the user cancels
and decides in the app; always save the state in `WM_ENDSESSION`. macOS: `NSTerminateLater`
with a non-blocking alert, `class_replaceMethod` instead of `class_addMethod`, smoke test in CI;
additionally `NSWorkspaceWillPowerOffNotification`. Linux: session inhibit while a timer is running.
The heartbeat fallback remains the actual guarantee; the dialogs are a convenience.

### 5. Session does not fit in the Windows Credential Manager — High

Finding: Blob limit 2560 bytes; the `keyring` crate checks the UTF-16 length (~1280 ASCII characters).
Jira Cloud sessions made up of several JWT cookies are considerably larger — saving fails. On
Linux, Secret Service is missing on minimal desktops and in containers.

Recommendation: Keep only a random 32-byte key in the keychain; store the session/tokens
encrypted with it (`chacha20poly1305`) in a file. Define a Linux fallback;
pin down the `keyring` feature flags.

### 6. Booking: cluster rounding vs. worklog per entry — High

Finding:
- TYME_ANALYSIS names the per-day/per-ticket aggregation as the basis for booking, but the model keeps
  `jira_worklog_id` per `time_entry`. 7 entries of 3 min. each, individually rounded to 15 min.,
  yield 105 instead of 21 min. It is unclear what happens to the worklog when a booked entry
  is changed or deleted.
- `issue_key` as FK: keys change when a ticket is moved; the ID is stable.
- `sprints.project_key`: sprints belong to boards; board filters can span several projects.
  Kanban and team-managed projects without a Scrum board return no sprints.
- Time zones: the `started_at` format is undefined. Day boundaries, target/actual, DST (23/25-hour days)
  need local time; Jira expects `started` with an offset (`…+0000`); Tempo Cloud expects
  `startDate`/`startTime` in the Jira profile time zone, which can differ from the local one.
- Jira v3 requires worklog comments as ADF, v2 (DC) as a string.
- Overlapping manual entries and multiple timers book the same wall-clock time twice.

Recommendation: Table `worklogs(id, issue_id, date, seconds_booked, jira_worklog_id,
tempo_worklog_id, synced_hash, remote_updated)` with n:1 `time_entries → worklogs`; rounding only at the
cluster level; a change to a booked entry marks the cluster as "diverged" → PUT/DELETE, after first
comparing the remote worklog's `updated`. `started_at`/`ended_at` as UTC epoch plus IANA `tz`;
Jira profile time zone from `/myself`. `issues.id` as PK. `sprints` attached to `boards`, projects→boards n:m.
Overlap warning; multiple timers only with a notice. Check how Jira handles `timeSpentSeconds < 60`
(enforce a minimum duration).

### 7. Jira client: endpoints and boards — Medium

Finding: `/rest/api/3/search` has been removed; `/search/jql` paginates via `nextPageToken`, returns
no `total` (only `/search/approximate-count`) and returns fields only with an explicit `fields`. A7
requires `/rest/agile/1.0/board?projectKeyOrId=` plus `/board/{id}/sprint?state=active,future`.

Recommendation: Cloud/DC behind a trait, `fields` always explicit, explain "project without a Scrum board" in
the UI, handle 429/`Retry-After` in the scheduler.

### 8. Corporate network: proxy and internal CAs — Medium

Finding: `reqwest` with rustls/webpki-roots does not trust internal CAs and TLS inspection — common in
SSO environments. The webview uses the system store, the Rust client does not: login works,
REST fails.

Recommendation: `rustls-tls-native-roots` (or `native-tls`), system proxy detection, optionally a
custom CA file in the settings.

### 9. Idle and sleep detection — Medium

Finding: `user-idle` is outdated; `user-idle3` supports Mutter D-Bus, X11 and
`ext-idle-notify-v1` (Mutter ≥ 45, KWin ≥ 5.27, Sway, Hyprland) — Wayland is better covered than
assumed. Detecting sleep via the tick interval is imprecise.

Recommendation: `user-idle3`; sleep/wake via OS signals (macOS `NSWorkspace`, Windows
`WM_POWERBROADCAST`, logind `PrepareForSleep`), watchdog only as a fallback.

### 10. No usable release without signing — Medium

Finding: Since macOS 15, unsigned downloads can no longer be opened via right-click;
SmartScreen warns. "Unsigned release at first" does not meet A13. Linux needs webkit2gtk-4.1.

Recommendation: Obtain Developer ID/notarization and a Windows certificate as a v0.1 prerequisite;
CI already in M1 so that platform problems become visible early.

### 11. Tauri details — Low

Window state is `tauri-plugin-window-state`, not `store`. The tray needs the `tray-icon` feature.
"Sign out" must clear the webview cookies (`clear_all_browsing_data`), otherwise switching accounts is impossible.
`window.print()` for PDF is limited in WebView2 — test early. Deep links (`yattt://`)
need `tauri-plugin-deep-link` plus single-instance.

### 12. Tyme matrix — Medium

- "Multiple timers: Yes" is problematic in the Jira context (double booking) → "Later" or with a warning.
- "Rounding global": Tyme rounds per project; a per-project override is cheap and useful.
- Missing: UI language/localization (not specified anywhere), edit duration directly, duplicate an entry/move
  it to another ticket, undo also for editing, daily target in the tray.
- "Import of existing worklogs" is right, but does not belong in v0.1.
- Favorites and "Open ticket in browser" are in the plan but missing from the matrix — align them.

### 13. Requirement A6 is deliberately not met literally — Medium

Finding: With SSO/webview or OAuth, the app never sees the password; "login and password stored
locally" is not met. That is correct, but it is not stated as a deviation in the plan.

Recommendation: Document explicitly and get confirmation: what is stored is the session or the
refresh token, never the password (exception: Basic against DC without SSO).

### 14. Milestones and v0.1 — Medium

Finding: A11 (mandatory) is in M5 "Convenience"; CI at the end; no auth spike.

Recommendation — order:
0. Auth spike (1 week): webview cookie and OAuth against the real instance, clarify the Tempo variant.
1. M1 including CI builds for three platforms.
2. M2 Jira. 3. M3 timer core, daily list, heartbeat/restore, idle.
4. M4 booking (without import), queue, status, Tempo direct path.
5. v0.1: signed builds, shutdown hooks at least for the team's platform.
6. Then statistics, export, reminders, import, weekly timeline, multiple timers.

Minimum for v0.1: one working login path, ticket list with sprint/type filter, one timer
(start/stop/switch), daily list with editing, booking with status, SQLite backup,
restore after a crash, tray with stop, signed packages from GitHub Actions.

### 15. Minor points — Low

SQLite in WAL mode, backups via `VACUUM INTO` instead of a file copy. Logging/diagnostics bundle is missing —
indispensable for auth problems in corporate networks. Reminders need a clear relation to the
working hours per weekday.
