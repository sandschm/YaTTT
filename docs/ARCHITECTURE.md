# Architektur

## Überblick

```
Svelte-Frontend (src/)  ── invoke/listen ──▶  Tauri-App (src-tauri/)  ──▶  yattt-core (crates/)
   Views, Store, Mock                     Commands, Tray, Login-Webview,       Jira/Tempo-Client, SQLite,
                                          Scheduler, Shutdown-Hooks            Timer, Sync, Stats, Export, Secrets
```

Die gesamte Fachlogik liegt in `yattt-core` und hat keine Tauri-Abhängigkeit. Das Crate ist mit
`cargo test` testbar (Unit-Tests, HTTP-Tests gegen `wiremock`). Die Tauri-Schicht ist dünn: sie hält
den Zustand (`AppState`), übersetzt Commands, startet Hintergrundaufgaben und spricht mit dem
Betriebssystem.

## yattt-core

| Modul | Aufgabe |
|---|---|
| `models` | Datenmodell (Project, Sprint, Issue, TimeEntry, Credential, …), serde camelCase |
| `settings` | `Settings` mit Defaults, JSON-Datei, `AppPaths` (Konfig-/Datenverzeichnis) |
| `secrets` | `EncryptedSecretStore`: Master-Key im Schlüsselbund, Nutzdaten XChaCha20-Poly1305 in `secrets.enc` |
| `jira` | `JiraClient` (Cloud/DC), `jira::parse` (JSON → Modell, Sprint-Feld, ADF), `jira::jql` (Query-Aufbau) |
| `tempo` | `TempoDataCenter` (über Jira-Session) und `TempoCloud` (Token) |
| `storage` | SQLite (`rusqlite`, WAL), Tabellen `projects`, `sprints`, `issues`, `time_entries`, `meta`; Backup per `VACUUM INTO` |
| `timer` | `TimerEngine`: Start/Stop/Umschalten, kurze Einträge verwerfen, Idle- und Wiederherstellungsentscheidungen |
| `sync` | Buchung (anlegen/aktualisieren/löschen) je nach `TempoMode`, Verarbeitung ausstehender Einträge, Import vorhandener Worklogs |
| `rounding` | Rundungsregeln für die Buchung |
| `stats` | Tagesstatistik mit lokalen Tagesgrenzen, Pausen aus Lücken, Soll aus Arbeitszeiten |
| `export` | CSV/JSON, Zusammenfassung pro Tag und Ticket |
| `idle` | Sekunden seit letzter Eingabe (plattformabhängig) |

### Zeitmodell

- Alle Zeitstempel werden als RFC 3339 in UTC gespeichert; Tagesgrenzen, Statistik und Anzeige
  verwenden die lokale Zeitzone. Einträge über Mitternacht werden für die Statistik geteilt.
- Ein laufender Timer ist ein `time_entry` ohne `ended_at`. Dadurch überlebt er Abstürze und
  Neustarts. `meta.last_heartbeat` wird alle 15 s geschrieben.
- Jira erhält `started` im Format `yyyy-MM-dd'T'HH:mm:ss.SSSZ` mit lokalem Offset; Mindestdauer 60 s.

### Buchungszustände

`local` → `pending` → `synced`; Änderung an einem gebuchten Eintrag → `modified` (wird per PUT
nachgezogen); Fehler → `failed` mit Fehlertext. Gelöschte Einträge mit Remote-ID werden erst nach
erfolgreichem Remote-DELETE endgültig entfernt.

## Tauri-Schicht

| Datei | Aufgabe |
|---|---|
| `lib.rs` | App-Aufbau, Plugins, Logging (Datei im Datenverzeichnis), Tastenkürzel, Exit-Behandlung |
| `state.rs` | `AppState`: Store, Settings, Client, Benutzer, Refresh-Status, Idle-Tracker, Wiederherstellungsdialog |
| `commands.rs` | Alle `#[tauri::command]`s |
| `jobs.rs` | Session wiederherstellen, Client installieren (inkl. Cookie-Rotation → Speicher), Tickets laden, Buchen |
| `login.rs` | Browser-Login-Fenster, Cookie-Polling, Logout inkl. Löschen der Webview-Daten |
| `scheduler.rs` | Tick (Heartbeat, Idle/Sleep, Tray-Titel, Erinnerungen), Auto-Refresh, Hintergrund-Sync, Backups |
| `tray.rs` | Tray-Menü (laufende Timer, Stop, zuletzt genutzte Tickets, Aktualisieren, Beenden), Titel mit Zeit |
| `shutdown/` | macOS `applicationShouldTerminate:` (NSAlert), Windows `WM_QUERYENDSESSION` (Block-Reason + App-Dialog) |

### Events (Backend → Frontend)

`issues-updated`, `timer-changed`, `entries-changed`, `auth-changed`, `refresh-status`,
`idle-prompt`, `exit-requested`, `settings-changed`, `sync-done`, `login-window`.

## Frontend

- Svelte 5 (Runes), kein Router: `store.view` schaltet zwischen Tickets, Zeiten, Statistik, Einstellungen.
- `lib/api.ts`: typisierte Wrapper um `invoke`; ohne Tauri (`npm run dev` im Browser) wird
  automatisch `lib/mock.ts` verwendet, ein In-Memory-Backend mit Beispieldaten
  (`window.__yatttMock.__simulateIdle()` und `__simulateRestore()` lösen die Dialoge aus).
- `lib/store.svelte.ts`: globaler Zustand, Event-Abos, Toasts, Sekundentakt für laufende Timer.
- Diagramme sind eigene SVGs (Balken, Donut, Heatmap); der Stundenzettel ist eine Druckansicht.

## Entscheidungen

Siehe [PLAN.md](PLAN.md), Abschnitt 9, für die Bewertung der Review-Findings und die daraus
abgeleiteten Entscheidungen (Cookie-Rotation, verschlüsselte Secrets, Tempo-Cloud-Pfad,
Shutdown-Hooks, Buchung pro Eintrag).
