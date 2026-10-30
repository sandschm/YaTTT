# Tyme-Analyse: Funktionsumfang und Übertragung auf YaTTT

Quelle: https://www.tyme-app.com/de/ und https://www.tyme-app.com/de/all-features/ (Stand 30.10.2026).

Tyme ist ein Zeiterfassungs-Tool für Apple-Plattformen (Mac, iPhone, iPad, Watch, Web-Beta).
YaTTT („Yet another Time Tracking Tool“) übernimmt den Funktionsumfang, der für eine
Desktop-App mit Jira als einziger Quelle für Projekte und Aufgaben sinnvoll ist.

Legende der Spalte „YaTTT“:

- **Ja** – wird in v1 umgesetzt
- **Angepasst** – wird umgesetzt, aber an Jira angepasst
- **Später** – sinnvoll, aber nicht Teil von v1
- **Nein** – passt nicht zum Konzept (Jira als Quelle, Einzelnutzer-Desktop-App)

## 1. Zeiterfassung & Timer

| Tyme | YaTTT | Umsetzung in YaTTT |
|---|---|---|
| Timer starten/stoppen pro Aufgabe | Ja | Timer pro Jira-Ticket, Start/Stop in Ticketliste, Tray und per Hotkey |
| Mehrere gleichzeitige Timer (Einstellung) | Ja | Einstellung „Mehrere Timer erlauben“, Standard: ein Timer, Start eines neuen stoppt den alten |
| Laufende Timer immer sichtbar | Ja | Timer-Leiste oben im Fenster, Tray-Titel (macOS) mit laufender Zeit |
| Startzeit nachträglich anpassen, rückdatieren, „so stoppen, als wäre früher Schluss gewesen“ | Ja | Laufender Timer: Startzeit editierbar, „Stoppen um …“ |
| Manuelle Zeiteinträge | Ja | Eintrag mit Ticket, Start, Ende, Notiz |
| Slot- vs. Cluster-Modus | Angepasst | Einträge werden immer als Slots (Start/Ende) gespeichert; Ansicht „Pro Tag und Ticket zusammenfassen“ liefert den Cluster-Blick und ist Grundlage der Buchung nach Jira/Tempo |
| Automatischer Timeout bei Inaktivität (Idle) | Ja | Idle-Erkennung (konfigurierbarer Schwellwert); bei Rückkehr Nachfrage: Zeit behalten / Idle-Zeit abziehen / Timer bei Idle-Beginn stoppen |
| Verhalten bei Ruhezustand/Abwesenheit | Ja | Sleep-Erkennung über Zeitsprung-Watchdog, gleiche Nachfrage wie bei Idle |
| Pausen automatisch erfassen | Angepasst | Pausen ergeben sich aus Lücken zwischen Einträgen und werden im Stundenzettel/Statistik ausgewiesen |
| Notizen beim Start/Stop (Dialog, optional) | Ja | Einstellung „Notiz beim Stoppen abfragen“; Notiz wird Worklog-Kommentar |
| Kurze Einträge ignorieren | Ja | Einstellung „Einträge unter N Sekunden verwerfen“ |
| Rundung (pro Projekt/Aufgabe) | Angepasst | Globale Rundungsregel (keine/5/6/10/15/30 min, auf/ab/kaufmännisch) für die Buchung nach Jira; lokale Zeiten bleiben exakt |
| Geofencing / Arbeitsort | Nein | Desktop-App ohne Standortdaten |
| Fahrtkosten, Ausgaben, Fixkosten, Pauschalen | Nein | Kein Abrechnungssystem, Jira/Tempo ist das Ziel |
| Globale Tastenkürzel | Ja | Letzten Timer starten/stoppen, Fenster ein-/ausblenden, Schnellstart |

## 2. Projekte, Aufgaben, Budgets

| Tyme | YaTTT | Umsetzung |
|---|---|---|
| Projekte, Aufgaben, Unteraufgaben, Kategorien | Angepasst | Jira-Projekt → Projekt, Jira-Ticket → Aufgabe, Jira-Subtask → Unteraufgabe, Epic → Gruppierung (optional ausblendbar) |
| Farben pro Projekt (8 Vorgaben + frei) | Ja | Farbe pro Jira-Projekt lokal wählbar, Standard aus Palette |
| Start-/Fälligkeitsdatum, Deadline-Hinweise | Angepasst | Due Date aus Jira anzeigen, Sprint-Ende als Deadline |
| Geplante Zeit / Budget, Soll-Ist-Balken | Angepasst | Original Estimate / Remaining aus Jira Timetracking als Budget; Balken „erfasst vs. geschätzt“ |
| Stundensätze, Umsatz, Währung | Nein | Nicht Aufgabe eines Jira-Trackers |
| Berechenbar / nicht berechenbar | Angepasst | Flag „Billable“ pro Eintrag (für Tempo relevant), Standard pro Projekt |
| Archiv | Angepasst | Projekt aus Auswahl entfernen = archivieren; erledigte Tickets werden ausgeblendet (Filter) |
| Teamfunktionen | Nein | Jira übernimmt Teamverwaltung |

## 3. Suche & Filter

| Tyme | YaTTT | Umsetzung |
|---|---|---|
| Globale Suche über Projekte/Aufgaben/Notizen | Ja | Suche in Ticketliste (Key, Titel), Notizen in Zeiteinträgen |
| Filter aktiv/archiviert, Aufgabentyp, berechenbar | Angepasst | Filter: Sprint, Tickettyp, Status, „nur mir zugewiesen“, „nur mit Zeiten“ |
| Zeitraum-Filter | Ja | Zeitraum in Zeiten- und Statistikansicht |
| Suche bei Projekt-/Sprint-Auswahl | Ja | Suchfelder in den Einstellungen (Anforderung) |

## 4. Berichte, Export, Statistik

| Tyme | YaTTT | Umsetzung |
|---|---|---|
| CSV-, JSON-, PDF-Export | Ja | CSV und JSON nativ; PDF über Druckansicht (Stundenzettel) mit Systemdialog „Als PDF sichern“ |
| Stundenzettel mit Pausen und Überstunden | Ja | Druckansicht pro Tag/Zeitraum mit Arbeitszeit, Pausen, Soll/Ist |
| Zusammenfassen: eine Zeile pro Aufgabe und Tag | Ja | Option beim Export |
| Buchhaltungsexporte (Lexware, sevdesk …) | Nein | Ziel ist Jira/Tempo |
| Statistik: Stunden pro Tag, Verteilung pro Projekt, Heatmap, Über-/Unterstunden, Pausen, Durchschnitt | Ja | Statistikansicht mit Balken (Tag), Verteilung (Projekt/Ticket), Heatmap (Jahr), Kennzahlen zu Arbeitszeit-Soll |
| Teamauslastung | Nein | – |

## 5. Kalender, Erinnerungen, Menüleiste

| Tyme | YaTTT | Umsetzung |
|---|---|---|
| Kalender-Sync (bidirektional) | Später | v1: Wochen-Timeline in der App (Einträge als Blöcke, verschieb- und editierbar) |
| Erinnerungen: Timer nicht gestartet, Tagesziel erreicht, Pause, nicht gebuchte Zeiten | Ja | Desktop-Benachrichtigungen: kein Timer nach Arbeitsbeginn, Tagesziel erreicht, Pausenhinweis nach N Stunden, ungebuchte Zeiten vorhanden, Timer läuft ungewöhnlich lange |
| Menüleiste: laufende Zeit, Tagessumme, aktuelle Aufgabe | Ja | Tray-Icon mit Menü (letzte Tickets, Stop, Fenster, Beenden); Titel mit Zeit auf macOS |
| Widgets, Apple Watch, Siri, Spotlight, Shortcuts | Nein | Plattformspezifisch |
| URL-Scheme / Automation | Später | `yattt://start/KEY` ist vorbereitet, nicht in v1 |
| MCP-Server, Plugin-API, AppleScript | Später | – |

## 6. Daten, Sync, Einstellungen

| Tyme | YaTTT | Umsetzung |
|---|---|---|
| iCloud-/Team-Sync | Angepasst | Jira/Tempo ist der Server. Lokal: SQLite als sichere Kopie aller Zeiten (Anforderung „sicherheitshalber lokal speichern“) |
| Import (Jira, CSV …) | Angepasst | Bestehende Worklogs des Nutzers aus Jira importieren (Zeitraum), damit Soll/Ist stimmt |
| Backup & Undo | Angepasst | Tägliches Backup der SQLite-Datei (rotierend); Undo für Löschen eines Eintrags |
| Hell/Dunkel | Ja | Systemfarbschema, umschaltbar |
| Start bei Anmeldung | Ja | Autostart-Plugin |
| Zeitformat (h:mm, dezimal) | Ja | Einstellung |
| Arbeitszeiten (flexibel pro Wochentag), erster Wochentag | Ja | Soll-Stunden pro Wochentag, Wochenstart für Statistik |

## 7. Jira-spezifische Funktionen (nicht in Tyme)

- Verbindung: Jira-URL, Anmeldung über eingebettetes Browserfenster (SSO, 2FA) oder Benutzername/Passwort, PAT oder API-Token als Rückfalloptionen
- Auswahl von Projekten und Sprints mit Suche; Tickets des aktiven Sprints aller gewählten Projekte werden immer geladen
- Filter für Tickettypen (z. B. Epics ausblenden)
- Automatische Aktualisierung der Ticketliste (Intervall konfigurierbar) und manueller Refresh
- Buchung der Zeiten als Worklog nach Jira (Tempo übernimmt sie automatisch) oder direkt über die Tempo-API; Status pro Eintrag (lokal / ausstehend / gebucht / Fehler), Wiederholung bei Fehlern
- Ticket im Browser öffnen
- Nachfrage bei Herunterfahren/Neustart/Beenden mit laufendem Timer; Wiederherstellung beim nächsten Start
