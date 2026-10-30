# Review zu PLAN.md und TYME_ANALYSE.md

Stand: 30.10.2026. Quellen stichprobenartig online geprüft (docs.rs/tauri, Atlassian-Doku, Tempo-KB,
Microsoft-Doku, tao-Quellcode).

## Gesamteinschätzung

Der Plan ist gut strukturiert, die Anforderungsmatrix nachvollziehbar, Architektur (`yattt-core`
ohne Tauri-Abhängigkeit) und Datenmodell sind solide Ausgangspunkte. Die Tyme-Analyse ist
sorgfältig, die Zuordnungen sind überwiegend plausibel.

Die Schwächen liegen dort, wo der Plan auf ungeprüften Annahmen aufbaut: (1) Cookie-Login als
einziger ernsthafter Auth-Pfad, (2) „Tempo übernimmt Jira-Worklogs automatisch“, (3) die
Shutdown-Dialoge unter Windows/macOS, (4) Session im OS-Schlüsselbund, (5) Buchungslogik im
Datenmodell. Alle sind lösbar, müssen aber vor M2 geklärt werden, weil sie Architektur und
Reihenfolge beeinflussen. Tauri 2 ist vertretbar (Finding 2), aber mit dem Auftraggeber bezüglich
„in Rust geschrieben“ abzustimmen.

## Findings

### 1. Cookie-Login als einziger Standardpfad ist zu riskant — Hoch

Befund: Cookie-Auth ist für Jira Cloud offiziell deprecated. Nicht adressiert:
- Identity Provider blockieren eingebettete Webviews: Google Workspace lehnt Logins aus Webviews
  ab (`disallowed_useragent`); Microsoft Entra Conditional Access („konformes Gerät“) scheitert in
  WebView2/WKWebView oft, weil das Browser-SSO-Plugin fehlt. Der IdP des Auftraggebers ist unbekannt.
- Atlassian rotiert Session-Cookies per `Set-Cookie`. Ein „manuell aus gespeicherter Session“
  gesetzter Cookie-Header verliert Rotationen; die Session läuft früher ab.
- Session-Dauer ist per Atlassian-Guard-Richtlinie steuerbar (Idle ab 15 Min., absolut z. B. 12 h).
- Das referenzierte `JIRA_AUTH.md` existiert nicht.

Empfehlung:
- Vorab klären: Cloud oder Data Center, welcher IdP, welches Tempo. Das entscheidet den Standardpfad.
- OAuth 2.0 als gleichwertigen, offiziellen Pfad einplanen, nicht nur als Token-Rückfall:
  Cloud: 3LO mit Systembrowser und `http://localhost:<port>`-Callback — SSO/2FA laufen im echten
  Browser. Einschränkungen: kein PKCE in Cloud (Client-Secret im Binary, bei Desktop-Apps
  akzeptiertes Risiko), App-Registrierung in der Developer Console, Org-Admins können 3LO sperren,
  Aufrufe über `api.atlassian.com/ex/jira/{cloudId}`. Data Center ≥ 8.22: OAuth 2.0 „Incoming
  Link“ mit PKCE, kein Secret; Redirect muss in Produktion HTTPS sein, localhost nur per
  System-Property (mit Jira-Admin klären).
- „Ohne API-Token“ gegenüber dem Auftraggeber präzisieren: gemeint ist wohl „kein manuell erzeugter
  Token“. OAuth erfüllt den Geist, braucht aber eine einmalige Admin-Registrierung.
- Cookie-Login behalten, aber mit Cookie-Jar (reqwest `cookie_store`) und Persistenz nach jeder
  Antwort. Vor M2 einen Spike gegen die echte Instanz mit SSO/2FA — das größte Projektrisiko.

### 2. Technologieentscheidung Tauri 2 + Svelte — Mittel

Befund: „Nur Tauri kann SSO/2FA“ gilt nur für den Webview-Cookie-Pfad; mit OAuth im Systembrowser
wäre auch ein reines Rust-GUI machbar. Tauri bleibt trotzdem pragmatisch: reife Tray-,
Notification-, Autostart-Plugins, HTML für Timeline/Heatmap/Druck, Webview-Login als Option.
`cookies_for_url` liefert laut docs.rs HttpOnly-Cookies auf allen Desktop-Plattformen; der
Windows-Deadlock in synchronen Commands ist korrekt erkannt.

Empfehlung: Entscheidung beibehalten, Begründung ehrlich fassen (Produktivität,
Plattform-Integration) und bestätigen lassen, dass „in Rust“ Fachlogik/Backend meint. Strikt
einhalten, dass `yattt-core` die gesamte Logik enthält.

### 3. Tempo Cloud übernimmt Jira-Worklogs erst über Nacht — Hoch

Befund: Laut Tempo-KB werden per Jira-REST erstellte Worklogs „overnight“ nach Tempo
synchronisiert; bei aktivem Tempo-Provider ist Jiras „Log Work“ deaktiviert; Tempo-Attribute
(Account, Billable) lassen sich über die Jira-API nicht setzen. Der „Standard“-Pfad liefert für
Tempo-Cloud-Nutzer bis zu einem Tag Verzögerung und unvollständige Daten. Für Tempo DC stimmt
die Annahme.

Empfehlung: Bei Tempo Cloud die Tempo-API als Standard. Tempo Cloud bietet OAuth 2.0
Authorization Code (Registrierung unter Settings → API Integration; Code läuft nach 60 s ab) —
kein manueller Token, aber Admin-Registrierung. Tempo DC wie geplant mit der Jira-Session.

### 4. Shutdown-Hooks funktionieren so nicht — Hoch

Befund:
- Windows: Eine `MessageBox` in `WM_QUERYENDSESSION` sieht der Nutzer nicht; seit Vista zeigt
  Windows einen Vollbild-Blockierschirm. „MessageBox, dann `TRUE`“ beendet ohne Entscheidung.
  `WM_ENDSESSION` kann auch nach `FALSE` kommen.
- macOS: tao implementiert `applicationShouldTerminate:` tatsächlich nicht (geprüft:
  `TaoAppDelegateParent`). `class_addMethod` funktioniert heute, schlägt aber still fehl, sobald
  tao die Methode selbst implementiert. Ein synchroner `NSAlert` blockiert den Logout.
- Linux: „nicht möglich“ ist zu pessimistisch. `org.gnome.SessionManager.Inhibit` (LOGOUT) zeigt
  die App mit Grund im End-Session-Dialog; KDE ähnlich. Nicht zuverlässig, aber besser als nichts.

Empfehlung: Windows: `ShutdownBlockReasonCreate("Timer läuft …")` + `FALSE`; der Nutzer bricht ab
und entscheidet in der App; in `WM_ENDSESSION` immer Zustand sichern. macOS: `NSTerminateLater`
mit nicht-blockierendem Alert, `class_replaceMethod` statt `class_addMethod`, Smoke-Test in CI;
zusätzlich `NSWorkspaceWillPowerOffNotification`. Linux: Session-Inhibit bei laufendem Timer.
Der Heartbeat-Fallback bleibt die eigentliche Garantie; die Dialoge sind Komfort.

### 5. Session passt nicht in den Windows Credential Manager — Hoch

Befund: Blob-Limit 2560 Byte; das `keyring`-Crate prüft die UTF-16-Länge (~1280 ASCII-Zeichen).
Jira-Cloud-Sessions aus mehreren JWT-Cookies sind deutlich größer — Speichern scheitert. Unter
Linux fehlt Secret Service auf minimalen Desktops und in Containern.

Empfehlung: Im Schlüsselbund nur einen zufälligen 32-Byte-Schlüssel; Session/Tokens damit
verschlüsselt (`chacha20poly1305`) in einer Datei ablegen. Linux-Fallback definieren;
`keyring`-Feature-Flags festlegen.

### 6. Buchung: Cluster-Rundung vs. Worklog pro Eintrag — Hoch

Befund:
- TYME_ANALYSE nennt die Tages-/Ticket-Zusammenfassung als Buchungsgrundlage, das Modell hält
  `jira_worklog_id` aber pro `time_entry`. 7 Einträge à 3 Min. einzeln auf 15 Min. gerundet
  ergeben 105 statt 21 Min. Unklar, was mit dem Worklog passiert, wenn ein gebuchter Eintrag
  geändert oder gelöscht wird.
- `issue_key` als FK: Keys ändern sich beim Verschieben; die ID ist stabil.
- `sprints.project_key`: Sprints gehören zu Boards; Board-Filter können mehrere Projekte umfassen.
  Kanban- und team-managed Projekte ohne Scrum-Board liefern keine Sprints.
- Zeitzonen: `started_at`-Format undefiniert. Tagesgrenzen, Soll/Ist, Sommerzeit (23/25-h-Tage)
  brauchen lokale Zeit; Jira erwartet `started` mit Offset (`…+0000`); Tempo Cloud erwartet
  `startDate`/`startTime` in der Jira-Profilzeitzone, die von der lokalen abweichen kann.
- Jira v3 verlangt Worklog-Kommentare als ADF, v2 (DC) als String.
- Überlappende manuelle Einträge und Mehrfach-Timer buchen dieselbe Wanduhrzeit doppelt.

Empfehlung: Tabelle `worklogs(id, issue_id, date, seconds_booked, jira_worklog_id,
tempo_worklog_id, synced_hash, remote_updated)` mit n:1 `time_entries → worklogs`; Rundung nur auf
Cluster-Ebene; Änderung an gebuchtem Eintrag markiert den Cluster „abweichend“ → PUT/DELETE, vorher
`updated` des Remote-Worklogs vergleichen. `started_at`/`ended_at` als UTC-Epoche plus IANA-`tz`;
Jira-Profilzeitzone aus `/myself`. `issues.id` als PK. `sprints` an `boards`, Projekte→Boards n:m.
Überlappungswarnung; Mehrfach-Timer nur mit Hinweis. Prüfen, wie Jira `timeSpentSeconds < 60`
behandelt (Mindestdauer erzwingen).

### 7. Jira-Client: Endpunkte und Boards — Mittel

Befund: `/rest/api/3/search` ist entfernt; `/search/jql` paginiert über `nextPageToken`, liefert
kein `total` (nur `/search/approximate-count`) und Felder nur bei explizitem `fields`. Für A7
braucht es `/rest/agile/1.0/board?projectKeyOrId=` plus `/board/{id}/sprint?state=active,future`.

Empfehlung: Cloud/DC hinter einem Trait, `fields` immer explizit, „Projekt ohne Scrum-Board“ in
der UI erklären, 429/`Retry-After` im Scheduler behandeln.

### 8. Unternehmensnetz: Proxy und interne CAs — Mittel

Befund: `reqwest` mit rustls/webpki-roots vertraut internen CAs und TLS-Inspection nicht — in
SSO-Umgebungen häufig. Der Webview nutzt den System-Store, der Rust-Client nicht: Login klappt,
REST scheitert.

Empfehlung: `rustls-tls-native-roots` (oder `native-tls`), System-Proxy-Erkennung, optional
eigene CA-Datei in den Einstellungen.

### 9. Idle- und Sleep-Erkennung — Mittel

Befund: `user-idle` ist veraltet; `user-idle3` unterstützt Mutter-D-Bus, X11 und
`ext-idle-notify-v1` (Mutter ≥ 45, KWin ≥ 5.27, Sway, Hyprland) — Wayland ist besser abgedeckt als
angenommen. Sleep über Tick-Abstand zu erkennen ist unscharf.

Empfehlung: `user-idle3`; Sleep/Wake über OS-Signale (macOS `NSWorkspace`, Windows
`WM_POWERBROADCAST`, logind `PrepareForSleep`), Watchdog nur als Fallback.

### 10. Ohne Signatur kein nutzbares Release — Mittel

Befund: Seit macOS 15 lassen sich unsignierte Downloads nicht mehr per Rechtsklick öffnen;
SmartScreen warnt. „Release ohne Signatur zunächst“ erfüllt A13 nicht. Linux braucht webkit2gtk-4.1.

Empfehlung: Developer ID/Notarisierung und Windows-Zertifikat als v0.1-Voraussetzung beschaffen;
CI schon in M1, damit Plattformprobleme früh sichtbar werden.

### 11. Tauri-Details — Niedrig

Fensterzustand ist `tauri-plugin-window-state`, nicht `store`. Tray braucht Feature `tray-icon`.
„Abmelden“ muss Webview-Cookies löschen (`clear_all_browsing_data`), sonst kein Kontowechsel.
`window.print()` für PDF ist in WebView2 eingeschränkt — früh testen. Deep-Links (`yattt://`)
brauchen `tauri-plugin-deep-link` plus Single-Instance.

### 12. Tyme-Matrix — Mittel

- „Mehrere Timer: Ja“ ist im Jira-Kontext problematisch (Doppelbuchung) → „Später“ oder mit Warnung.
- „Rundung global“: Tyme rundet pro Projekt; Projekt-Überschreibung ist billig und sinnvoll.
- Fehlend: UI-Sprache/Lokalisierung (nirgends festgelegt), Dauer direkt editieren, Eintrag
  duplizieren/auf anderes Ticket verschieben, Undo auch für Bearbeiten, Tagesziel im Tray.
- „Import vorhandener Worklogs“ ist richtig, gehört aber nicht in v0.1.
- Favoriten und „Ticket im Browser öffnen“ stehen im Plan, fehlen in der Matrix — angleichen.

### 13. Anforderung A6 wird bewusst nicht wörtlich erfüllt — Mittel

Befund: Bei SSO/Webview oder OAuth sieht die App das Passwort nie; „Login und Passwort lokal
gespeichert“ wird nicht erfüllt. Das ist richtig, steht aber nicht als Abweichung im Plan.

Empfehlung: Explizit dokumentieren und bestätigen lassen: gespeichert wird Session bzw.
Refresh-Token, nie das Passwort (Ausnahme: Basic gegen DC ohne SSO).

### 14. Meilensteine und v0.1 — Mittel

Befund: A11 (Pflicht) liegt in M5 „Komfort“; CI am Ende; kein Auth-Spike.

Empfehlung — Reihenfolge:
0. Spike Auth (1 Woche): Webview-Cookie und OAuth gegen die echte Instanz, Tempo-Variante klären.
1. M1 inkl. CI-Builds für drei Plattformen.
2. M2 Jira. 3. M3 Timer-Kern, Tagesliste, Heartbeat/Wiederherstellung, Idle.
4. M4 Buchung (ohne Import), Queue, Status, Tempo-Direktpfad.
5. v0.1: signierte Builds, Shutdown-Hooks mindestens für die Plattform des Teams.
6. Danach Statistik, Export, Erinnerungen, Import, Wochen-Timeline, Mehrfach-Timer.

Minimal für v0.1: ein funktionierender Login-Pfad, Ticketliste mit Sprint-/Typ-Filter, ein Timer
(Start/Stop/Umschalten), Tagesliste mit Bearbeiten, Buchung mit Status, SQLite-Sicherung,
Wiederherstellung nach Absturz, Tray mit Stop, signierte Pakete aus GitHub Actions.

### 15. Kleinere Punkte — Niedrig

SQLite im WAL-Modus, Backups per `VACUUM INTO` statt Dateikopie. Logging/Diagnosepaket fehlt —
für Auth-Probleme im Unternehmensnetz unverzichtbar. Erinnerungen brauchen klaren Bezug zu den
Arbeitszeiten pro Wochentag.
