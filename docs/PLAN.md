# YaTTT – Plan und Architektur

YaTTT („Yet another Time Tracking Tool“) ist eine Desktop-App zur Zeiterfassung an Jira-Tickets
mit Buchung nach Jira/Tempo und lokaler Sicherungskopie. Funktionsumfang orientiert sich an Tyme
(siehe [TYME_ANALYSE.md](TYME_ANALYSE.md)). Zielplattformen: macOS, Windows, Linux.

## 1. Anforderungen (aus der Aufgabenstellung)

| # | Anforderung | Lösung |
|---|---|---|
| A1 | Zeiterfassung an Jira-Tickets, Tempo vorhanden | Worklogs über Jira-REST (Tempo synchronisiert automatisch) oder Tempo-API |
| A2 | Zusätzlich lokale Speicherung | SQLite im Nutzer-Datenverzeichnis, tägliches Backup |
| A3 | Funktionsumfang wie Tyme | Siehe Feature-Matrix in TYME_ANALYSE.md |
| A4 | Alle Aufgaben und Projekte aus Jira | Kein lokales Anlegen von Projekten/Aufgaben; Tickets werden gecacht |
| A5 | Einstellungen: Jira-URL | Einstellungsseite „Verbindung“ |
| A6 | Login/Passwort beim Login abfragen, sicher lokal speichern | Anmeldung im eingebetteten Browserfenster (SSO/2FA) oder Formular; Session/Token im OS-Schlüsselbund (`keyring`), nie im Klartext in Dateien |
| A7 | Auswahl von Projekten und Sprints, mit Suche | Einstellungen „Projekte & Sprints“ mit Suchfeldern |
| A8 | Tickettypen herausfiltern (z. B. Epics) | Einstellung „Ausgeblendete Tickettypen“ |
| A9 | Tickets des aktuellen Sprints aller gewählten Projekte immer laden | JQL `project in (…) AND sprint in openSprints()` zusätzlich zu gewählten Sprints |
| A10 | Regelmäßige Aktualisierung, Intervall konfigurierbar, manuell anstoßbar | Scheduler im Rust-Backend, Refresh-Button |
| A11 | Herunterfahren/Neustart bei laufendem Timer: Nachfrage stoppen/weiterlaufen | Native Hooks (macOS `applicationShouldTerminate:`, Windows `WM_QUERYENDSESSION`), überall zusätzlich Wiederherstellungsdialog beim nächsten Start |
| A12 | Rust, macOS/Windows/Linux | Tauri 2 (Rust-Backend) + Rust-Core-Crate; UI als Svelte/TypeScript |
| A13 | Release-Pipeline auf GitHub | GitHub Actions: CI (Test/Lint) und Release (tauri-action, 3 Plattformen) |
| A14 | API-Zugriff ohne API-Token, SSO und 2FA | Browser-Session-Login in Webview, Cookies werden übernommen; siehe Abschnitt 4 |

## 2. Technologieentscheidung

### Warum Tauri 2 und nicht ein reines Rust-GUI (egui/iced/Slint)?

Die Anforderung A14 (SSO + zweiter Faktor, kein API-Token) ist nur erfüllbar, wenn der Nutzer
den echten Jira-Login-Dialog (inkl. Weiterleitung zum Identity Provider und 2FA) durchlaufen kann.
Dafür braucht die App einen eingebetteten Browser, aus dem sie anschließend die Session-Cookies
übernimmt. Tauri 2 bietet genau das: `Webview::cookies_for_url()` liefert auch HttpOnly-Cookies
auf allen drei Plattformen (WKWebView, WebView2, WebKitGTK). Reine Rust-GUIs haben keinen
eingebetteten Browser; ein Login würde dort nur mit API-Token/PAT funktionieren.

Konsequenz: Die gesamte Fachlogik (Jira-Client, Tempo, Timer, Speicherung, Export, Idle, Scheduler,
Shutdown-Hooks) liegt in Rust. Die Oberfläche ist HTML/Svelte/TypeScript und spricht ausschließlich
über Tauri-Commands und -Events mit dem Backend. Die Rust-Logik ist in einem eigenen Crate
(`yattt-core`) ohne Tauri-Abhängigkeit und daher mit `cargo test` testbar.

Alternative, bewusst nicht gewählt: Leptos/Yew als Rust→WASM-Frontend. Vorteil: 100 % Rust.
Nachteil: deutlich langsamere Entwicklung und Toolchain-Komplexität (Trunk, wasm32) bei einer
UI mit vielen Ansichten. Kann später ersetzt werden, da die Schnittstelle (Commands/Events) stabil ist.

### Stack

| Bereich | Wahl |
|---|---|
| App-Rahmen | Tauri 2 (tray-icon, Menüs, Webview-Login) |
| Backend-Logik | Rust, Crate `yattt-core` |
| HTTP | `reqwest` (rustls), Cookie-Header manuell aus gespeicherter Session |
| Speicherung | `rusqlite` (bundled), Migrationen im Code |
| Einstellungen | JSON im Konfigurationsverzeichnis (`dirs`) |
| Geheimnisse | `keyring` (macOS Keychain, Windows Credential Manager, Linux Secret Service) |
| Idle-Erkennung | `user-idle` (CGEventSource / GetLastInputInfo / X11), Wayland: Zeitsprung-Watchdog |
| Scheduler | `tokio` Tasks im Tauri-Runtime |
| Frontend | Svelte 5, TypeScript, Vite; Diagramme als eigene SVG-Komponenten |
| Tauri-Plugins | notification, dialog, opener, autostart, global-shortcut, single-instance, store (Fensterzustand) |
| Tests | `cargo test` (Unit), `wiremock` (Jira-Client gegen Mock-Server), `vitest` (Frontend-Helfer) |
| CI/CD | GitHub Actions, `tauri-apps/tauri-action` |

## 3. Architektur

```
┌──────────────────────────── Tauri-App (src-tauri) ─────────────────────────────┐
│  commands.rs  – dünne Tauri-Commands, mappen auf yattt-core                     │
│  events       – timer-tick, issues-updated, sync-status, auth-expired, …        │
│  tray.rs      – Tray-Icon/-Menü, Titel mit laufender Zeit                        │
│  login.rs     – Login-Webview öffnen, Cookies pollen, Session prüfen             │
│  shutdown/    – macOS applicationShouldTerminate:, Windows WM_QUERYENDSESSION    │
│  scheduler.rs – Refresh-Intervall, Idle-Watchdog, Erinnerungen, Sync-Retry       │
└────────────────────────────────────┬───────────────────────────────────────────┘
                                     │ Rust-API
┌────────────────────────────── yattt-core ──────────────────────────────────────┐
│  settings   – Settings-Struct, Laden/Speichern, Defaults, Validierung            │
│  auth       – AuthMethod (Session-Cookies, Basic, PAT, ApiToken), SecretStore    │
│  jira       – JiraClient: serverInfo, myself, projects, boards, sprints, search, │
│               issue types, worklog create/update/delete; Cloud/DC-Unterschiede   │
│  tempo      – TempoClient (DC: /rest/tempo-timesheets/4, Cloud: api.tempo.io/4)  │
│  storage    – SQLite: projects, issues, time_entries, sync_queue, meta; Backups  │
│  timer      – TimerEngine: start/stop/switch, Idle-Handling, Rundung             │
│  sync       – Worklog-Buchung, Retry, Import vorhandener Worklogs                │
│  export     – CSV/JSON, Stundenzettel-Daten (Tage, Pausen, Soll/Ist)             │
│  stats      – Aggregationen für Statistikansicht                                 │
│  idle       – Plattformabstraktion für Idle-Zeit                                 │
└────────────────────────────────────────────────────────────────────────────────┘
                                     ▲ Tauri invoke/listen
┌────────────────────────── Frontend (src, Svelte) ──────────────────────────────┐
│  Ansichten: Timer-Leiste · Tickets · Zeiten (Liste/Woche) · Statistik ·          │
│             Einstellungen (Verbindung, Projekte & Sprints, Filter, Zeit, Verhalten)│
│  Dialoge:   Login, Idle-Rückkehr, Beenden mit laufendem Timer, Wiederherstellung  │
└────────────────────────────────────────────────────────────────────────────────┘
```

### Datenmodell (SQLite)

- `projects(key PK, id, name, avatar_url, color, selected, billable_default, sort)`
- `sprints(id PK, board_id, name, state, start_date, end_date, project_key, selected)`
- `issues(key PK, id, project_key, summary, issue_type, issue_type_id, status, status_category, parent_key, epic_key, sprint_id, assignee, due_date, original_estimate_s, remaining_estimate_s, time_spent_s, updated, url, last_seen, favorite)`
- `time_entries(id PK, issue_key, started_at, ended_at NULL, note, billable, rounded_seconds NULL, sync_state, jira_worklog_id, tempo_worklog_id, sync_error, created_at, updated_at, deleted_at)`
- `sync_queue(id, entry_id, action, attempts, next_attempt_at, last_error)`
- `meta(key, value)` – Schema-Version, letzte Aktualisierung, letzter sauberer Exit, laufende Timer beim Exit

Geheimnisse (Cookies, Tokens, Passwort bei Basic-Auth) liegen ausschließlich im Schlüsselbund
unter dem Service `de.yattt.app`, Account = Jira-URL.

### Timer-Semantik

- Ein Zeiteintrag mit `ended_at = NULL` ist ein laufender Timer. Dadurch überlebt der Timer App-Abstürze,
  Neustarts und Shutdown: Beim nächsten Start findet die App offene Einträge und fragt nach
  („bis jetzt weiterlaufen lassen“ / „zum Zeitpunkt des Beendens stoppen“ / „zum Zeitpunkt X stoppen“).
- `meta.last_heartbeat` wird alle 15 s geschrieben. Beim Start ist damit der Zeitpunkt des letzten
  Lebenszeichens bekannt, falls der Rechner ohne Nachfrage ausging.
- Einzel-Timer-Modus: Start eines Timers stoppt den laufenden. Mehrfach-Timer optional.
- Idle: Bei Idle > Schwellwert merkt sich das Backend den Idle-Beginn; nach Rückkehr Dialog mit
  drei Optionen. Sleep wird erkannt, wenn der Tick-Abstand den erwarteten Takt deutlich überschreitet.
- Rundung wird nur bei der Buchung angewandt; lokale Daten bleiben sekundengenau.

### Shutdown/Neustart/Beenden

| Plattform | Mechanismus | Verhalten |
|---|---|---|
| macOS | Eigene Implementierung von `applicationShouldTerminate:` am tao-App-Delegate (zur Laufzeit per `class_addMethod`), da tao die Methode nicht implementiert | Bei laufendem Timer nativer `NSAlert` („Timer stoppen“ / „Weiterlaufen lassen“), danach `NSTerminateNow`; ohne Timer sofort beenden |
| Windows | Subclassing des Hauptfensters, `WM_QUERYENDSESSION` + `ShutdownBlockReasonCreate` | `MessageBox` mit denselben Optionen, dann Rückgabe `TRUE` |
| Linux | Kein zuverlässiger Dialog während des Shutdowns (logind-Delay-Lock max. 5 s) | Zustand ist durch Heartbeat gesichert; Nachfrage beim nächsten Start |
| Alle | `RunEvent::ExitRequested` (Cmd+Q, Tray „Beenden“) | In-App-Dialog „Timer stoppen und beenden“ / „Weiterlaufen lassen und beenden“ / „Abbrechen“ |
| Alle | Fenster schließen | Minimiert in den Tray (Einstellung) |

„Weiterlaufen lassen“ bedeutet: der Eintrag bleibt offen und zählt weiter; beim nächsten Start
läuft der Timer sichtbar weiter, mit Hinweis.

## 4. Jira-Zugriff ohne API-Token, mit SSO und 2FA

### Befund

- **Jira Cloud:** Basic-Auth mit Passwort ist seit 2019 abgeschaltet; es bleiben API-Token, OAuth 2.0 (3LO)
  oder die Browser-Session (`cloud.session.token`, HttpOnly). Der Login läuft immer über
  `id.atlassian.com`/`auth.atlassian.com`; SSO (SAML via Atlassian Guard) und 2FA passieren dort.
  Test: `https://jira.atlassian.com/login.jsp` leitet auf `auth.atlassian.com/authorize?...` weiter.
  Atlassian bezeichnet Cookie-Auth für externe Clients als „deprecated“, der Browser selbst nutzt aber
  genau diese Session für alle REST-Aufrufe. Eine App, die den Browser-Login einbettet, verhält sich wie der Browser.
- **Jira Data Center/Server:** Session über `POST /rest/auth/1/session` (Benutzername/Passwort) liefert `JSESSIONID`.
  Mit SAML/Crowd-SSO und 2FA-Plugins funktioniert dieser Weg häufig nicht (Login wird zum IdP umgeleitet).
  Der Browser-Login setzt nach erfolgreichem SSO/2FA `JSESSIONID` (und ggf. `seraph.rememberme.cookie`),
  die für REST-Aufrufe gültig sind. PATs (ab 8.14) sind eine saubere Alternative, sind aber Tokens.
- Getestet gegen öffentliche Instanzen (anonym): ASF Jira (DC 10.3) liefert `serverInfo`, `project`, `search`;
  `myself` und `agile/board` verlangen Authentifizierung (401). Jira Cloud (`jira.atlassian.com`) verlangt für
  REST ohne Session einen Login (302). Die API-Formen für Cloud (`/rest/api/3/search/jql`, `nextPageToken`)
  und DC (`/rest/api/2/search`, `startAt`) unterscheiden sich und werden im Client abstrahiert.

### Lösung: Browser-Session-Login (Standard)

1. Nutzer gibt Jira-URL ein und klickt „Anmelden“.
2. Die App öffnet ein Tauri-Webview-Fenster mit `<jira-url>/login.jsp?os_destination=/rest/api/2/myself`
   (Cloud: Weiterleitung zu Atlassian-ID, dann IdP, dann 2FA, zurück zur Site).
3. Das Backend prüft alle 1,5 s: `cookies_for_url(jira-url)` lesen, mit diesen Cookies
   `GET /rest/api/2/myself` aufrufen. Antwort 200 → Login erfolgreich.
4. Cookies (Name, Wert, Domain, Pfad, Ablauf) werden als Session im Schlüsselbund gespeichert, das Fenster schließt sich.
5. Alle REST-Aufrufe senden `Cookie: …` und `X-Atlassian-Token: no-check` (XSRF-Ausnahme für
   Nicht-Formular-Requests). Bei 401/403 wird `auth-expired` gemeldet und „Erneut anmelden“ angeboten;
   die Session-Cookies im Webview-Profil bleiben erhalten, sodass die Neuanmeldung oft ohne erneute
   Passworteingabe gelingt.

Rückfalloptionen in den Einstellungen: Benutzername/Passwort (DC, `/rest/auth/1/session` bzw. Basic),
Personal Access Token (DC), E-Mail + API-Token (Cloud). Passwort wird nur im Schlüsselbund gespeichert.

### Teststrategie für Auth

- Unit/Integrationstests des Clients gegen `wiremock` (Session-Cookie, 401-Handling, Cloud/DC-Pagination).
- Manueller Test der Webview-Cookie-Übernahme gegen `issues.apache.org/jira` (setzt `JSESSIONID` auch anonym).
- Der echte SSO/2FA-Test ist nur gegen die Jira-Instanz des Teams möglich (Checkliste in [JIRA_AUTH.md](JIRA_AUTH.md)).

## 5. Tempo

- **Standard:** Worklog über `POST /rest/api/2/issue/{key}/worklog` (`started`, `timeSpentSeconds`, `comment`).
  Tempo Cloud übernimmt Jira-Worklogs per Webhook, Tempo DC speichert Worklogs ohnehin in Jira.
- **Tempo DC direkt (optional):** `POST /rest/tempo-timesheets/4/worklogs` mit derselben Jira-Session
  (`originTaskId`, `started`, `timeSpentSeconds`, `billableSeconds`, `worker`, `comment`).
- **Tempo Cloud direkt (optional):** `POST https://api.tempo.io/4/worklogs` mit Tempo-Token
  (`issueId`, `authorAccountId`, `startDate`, `startTime`, `timeSpentSeconds`, `billableSeconds`).
- Jeder Eintrag merkt sich `jira_worklog_id`/`tempo_worklog_id`; Änderungen am Eintrag führen zu
  `PUT`, Löschen zu `DELETE`. Fehler landen in der `sync_queue` mit Backoff.

## 6. Oberfläche

- **Timer-Leiste:** aktueller Timer (Ticket, Zeit, Notiz), Stop, Tagessumme, Schnellstart-Suche.
- **Tickets:** Gruppen pro Projekt (Farbe), Sprint-Badge, Typ-Icon, Status, Soll/Ist-Balken,
  Start-Button, Favoriten (max. 15, Hinweisdialog darüber), Filterleiste (Sprint, Typ, Komponente, Label, Status, nur meine), Suche, Refresh mit Zeitstempel.
- **Tray:** Favoriten als Check-Einträge; Klick startet bzw. stoppt den Timer, das Häkchen zeigt den laufenden.
- **Filter je Projekt:** `selection.projectFilters` (Projekt → Komponenten, Labels, und/oder) ergänzt das JQL um
  `(project not in (…) OR (project = X AND (component in (…) AND|OR labels in (…))))`, sodass nur das
  betroffene Projekt gefiltert wird. Komponenten kommen aus `/rest/api/2/project/{key}/components`.
- **Zeiten:** Tagesliste mit Einträgen und Tagessummen, Bearbeiten/Löschen, Buchungsstatus,
  „Jetzt buchen“; Wochen-Timeline mit Blöcken; Pausen als Lücken.
- **Statistik:** Zeitraumwahl; Kennzahlen (Soll/Ist, Über-/Unterstunden, Pausen, Billable-Anteil);
  Balken pro Tag; Verteilung pro Projekt/Ticket; Heatmap.
- **Einstellungen:** Verbindung (URL, Login-Methode, Status, Abmelden) · Projekte & Sprints (Suche,
  Auswahl) · Filter (Tickettypen, erledigte ausblenden) · Zeit (Rundung, Arbeitszeiten pro Wochentag,
  Wochenstart, Zeitformat, kurze Einträge) · Verhalten (Intervall, Idle-Schwelle, Mehrfach-Timer,
  Autostart, in Tray minimieren, Benachrichtigungen, Notiz beim Stoppen, Buchen beim Stoppen) ·
  Tempo (Modus, Token) · Export.

## 7. Meilensteine

1. **M1 Grundgerüst:** Workspace, `yattt-core` mit Settings/Storage/Models, Tauri-Shell, Fenster, Tray.
2. **M2 Jira:** Client (Cloud/DC), Login-Webview, Schlüsselbund, Projekte/Sprints/Issues laden, Scheduler.
3. **M3 Zeiterfassung:** TimerEngine, Einträge, Idle/Sleep, Zeiten-Ansicht, Timer-Leiste, Hotkeys.
4. **M4 Buchung:** Worklogs nach Jira/Tempo, Queue, Statusanzeige, Import vorhandener Worklogs.
5. **M5 Komfort:** Statistik, Export, Erinnerungen, Shutdown-Hooks, Wiederherstellung.
6. **M6 Auslieferung:** Tests, CI, Release-Pipeline, Dokumentation.

## 8. Risiken und Gegenmaßnahmen

| Risiko | Gegenmaßnahme |
|---|---|
| Cookie-Login wird von Atlassian oder der Instanz blockiert (Captcha, WAF) | Fallback PAT/API-Token; Fehler klar anzeigen |
| Session läuft ab (Cloud: einige Tage) | 401-Erkennung, Banner „Erneut anmelden“, Webview behält Browser-Session |
| XSRF-Prüfung bei POST mit Cookie-Auth | `X-Atlassian-Token: no-check`, JSON-Content-Type, kein `Origin`-Header |
| `Webview::cookies()` kann unter Windows in synchronen Commands blockieren | Nur `cookies_for_url` aus async Tasks aufrufen |
| Tray/Hotkeys unter Wayland eingeschränkt | Dokumentieren; App bleibt ohne Tray nutzbar |
| Idle-Erkennung unter Wayland nicht verfügbar | Zeitsprung-Watchdog bleibt aktiv, Idle optional abschaltbar |
| Shutdown-Dialog unter Linux nicht möglich | Heartbeat + Nachfrage beim nächsten Start |
| Cloud-Suche nutzt neue Pagination | Client erkennt `deploymentType` und wählt Endpunkt |
| Code-Signing fehlt | Release ohne Signatur zunächst; Secrets für Signierung vorbereitet und dokumentiert |

## 9. Review-Ergebnis und Entscheidungen

Der Plan wurde unabhängig geprüft, siehe [PLAN_REVIEW.md](PLAN_REVIEW.md) (15 Findings). Entscheidungen:

| Finding | Entscheidung |
|---|---|
| 1 Cookie-Login als einziger Pfad | Browser-Login bleibt Standard (Anforderung: kein API-Token, SSO, 2FA). Ergänzt: Cookie-Rotation wird übernommen (jede `Set-Cookie`-Antwort aktualisiert die gespeicherte Session), Login-Webview mit Desktop-Browser-User-Agent (IdPs blocken sonst eingebettete Webviews). OAuth 2.0 (3LO, Systembrowser + localhost-Callback) ist als zweiter offizieller Pfad vorgesehen (Credential-Variante im Modell, Umsetzung nach v0.1, da App-Registrierung durch den Auftraggeber nötig). Ein Spike gegen die echte Instanz ist der erste Schritt nach Übergabe, siehe [JIRA_AUTH.md](JIRA_AUTH.md). |
| 2 Tauri statt reinem Rust-GUI | Beibehalten. Begründung ehrlich: Produktivität, Plattformintegration, Webview-Login. Fachlogik vollständig in `yattt-core`. Muss vom Auftraggeber bestätigt werden. |
| 3 Tempo Cloud synchronisiert Jira-Worklogs nur über Nacht | Bei Jira Cloud wird der Tempo-Cloud-API-Pfad (Tempo-Token) als Standard empfohlen und in der UI vorgeschlagen; Jira-Worklog bleibt Standard für Data Center. |
| 4 Shutdown-Hooks | macOS: `applicationShouldTerminate:` per `class_replaceMethod` (fügt hinzu oder ersetzt, robust gegen künftige tao-Versionen), synchroner `NSAlert`. Windows: `WM_QUERYENDSESSION` → `ShutdownBlockReasonCreate` + `FALSE`, Entscheidung im App-Dialog, `WM_ENDSESSION` sichert Zustand. Linux: Heartbeat und Nachfrage beim Start (Session-Inhibit später). Heartbeat bleibt die Garantie auf allen Plattformen. |
| 5 Session passt nicht in Windows Credential Manager | Übernommen: im Schlüsselbund liegt nur ein zufälliger 32-Byte-Schlüssel; Session, Tokens und Passwörter liegen XChaCha20-Poly1305-verschlüsselt in `secrets.enc` im Konfigurationsverzeichnis. Ohne Schlüsselbund (Linux ohne Secret Service) liegt der Schlüssel in einer Datei mit Rechten 0600, mit Warnung in den Einstellungen. |
| 6 Buchung pro Eintrag vs. Cluster | v0.1 bucht pro Eintrag (nachvollziehbar, einfache Korrektur per PUT/DELETE). Rundung ist standardmäßig aus; Hinweis in den Einstellungen, dass Rundung pro Eintrag wirkt. Cluster-Buchung (ein Worklog pro Tag und Ticket) ist als Folgearbeit eingeplant. Zeitstempel: UTC in SQLite, lokale Zeit für Tagesgrenzen, Jira `started` mit Offset. Mindestdauer 60 s wird erzwungen. Mehrfach-Timer sind standardmäßig aus. Sprints hängen an Boards, nicht an Projekten. |
| 7 Endpunkte | Cloud `/rest/api/2/search/jql` mit `nextPageToken`, DC `/rest/api/2/search` mit `startAt`; Felder immer explizit; 429 mit `Retry-After` wird einmal wiederholt. |
| 8 Interne CAs/Proxy | `reqwest` mit `rustls-tls-native-roots` (System-Zertifikatspeicher), System-Proxy über Umgebungsvariablen. |
| 9 Idle unter Wayland | `user-idle` bleibt (X11/macOS/Windows); Wayland über Zeitsprung-Watchdog. Wechsel auf `user-idle3` geprüft, siehe Risiken. |
| 10 Signierung | Pipeline unterstützt Signierung/Notarisierung über Secrets (`APPLE_CERTIFICATE`, `APPLE_ID`, `TAURI_SIGNING_PRIVATE_KEY`); ohne Secrets werden unsignierte Artefakte gebaut. Zertifikate muss der Auftraggeber bereitstellen. |
| 11 Tauri-Details | `tauri-plugin-window-state`, Feature `tray-icon`, „Abmelden“ löscht Webview-Daten (`clear_all_browsing_data`). |
| 12 Tyme-Matrix | Mehrfach-Timer mit Warnung, Dauer direkt editierbar, Eintrag duplizieren/verschieben in der Zeitenansicht. UI-Sprache: Deutsch (v0.1). |
| 13 A6 | Abweichung dokumentiert: Bei SSO wird nie das Passwort, sondern die Session gespeichert. Nur bei Benutzername/Passwort gegen Data Center ohne SSO liegt das Passwort (verschlüsselt) lokal. |
| 14 Reihenfolge | CI ab M1; Heartbeat/Wiederherstellung in M3; Shutdown-Hooks in M4 statt M5; Import vorhandener Worklogs nach v0.1. |
| 15 Kleinigkeiten | WAL ist aktiv, Backup per SQLite-Backup-API, Logdatei im Datenverzeichnis. |

## 10. Umsetzungsstand (31.10.2026)

| Bereich | Stand |
|---|---|
| `yattt-core` | Umgesetzt: Settings, verschlüsselte Secrets, Jira-Client (Cloud/DC, Cookie-Rotation, 429-Retry), JQL-Aufbau, Tempo DC/Cloud, SQLite, TimerEngine, Buchung/Sync/Import, Statistik, Export, Rundung, Idle. 47 Tests grün, Clippy ohne Warnungen. |
| Tauri-App | Umgesetzt: Commands, Login-Webview mit Cookie-Übernahme, Tray (Menü, Titel), Scheduler (Heartbeat, Idle/Sleep, Auto-Refresh, Hintergrund-Sync, Erinnerungen, Backups), globale Tastenkürzel, Autostart, Shutdown-Hooks macOS/Windows, Wiederherstellungsdialog. |
| Frontend | Umgesetzt: Timer-Leiste, Tickets (Gruppen, Filter inkl. Labels, Suche, Favoriten mit Limit, Soll/Ist), Zeiten (Liste, Woche, Editor, Buchen, Undo), Statistik (KPIs, Balken, Donut, Top-Tickets, Heatmap, Export, Stundenzettel), Einstellungen (7 Bereiche), Dialoge (Idle, Wiederherstellung, Beenden, Notiz). Mock-Backend für Entwicklung im Browser. |
| CI/Release | CI läuft auf GitHub grün (ubuntu-22.04, macOS, Windows). Release-Workflow baut Linux (.deb/.rpm/.AppImage), Windows (.msi/-setup.exe) und macOS (universelles .dmg); Signierung nur, wenn Secrets gesetzt sind. Erstes vollständiges Release: v0.1.1; v0.2.0 mit Tray-Favoriten und Label-Filter (31.10.2026). |
| Getestet | Core-Tests; UI im Browser (Mock); native App auf macOS: Shutdown-Hook installiert, Backup geschrieben, SSO/2FA-Login gegen die Jira-Cloud-Instanz des Teams bestätigt (31.10.2026); Tray-Favoriten und Label-Filter in der nativen App bestätigt. |
| Offen | Buchung gegen die Jira-Instanz des Teams noch nicht im Echtbetrieb geprüft; Windows-/Linux-Builds nur über CI geprüft; Windows-Shutdown-Hook ungetestet; OAuth 2.0, Cluster-Buchung, Linux-Session-Inhibit als Folgearbeiten. |
