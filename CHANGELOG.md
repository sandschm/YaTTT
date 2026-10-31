# Änderungen

## 0.3.0 – 2026-10-31

- Filter je Projekt: Neben Labels lassen sich jetzt Komponenten hinterlegen (Komponentenliste
  des Projekts als Vorschlag, Leerzeichen erlaubt). Sind beides gesetzt, wählt man je Projekt
  „Komponente und Label“ oder „Komponente oder Label“. Alte Label-Einstellungen werden übernommen.
- Ticketliste zeigt Komponenten als Chips und bietet einen Komponenten-Filter.
- Datenbankschema 3: Spalte `components` an Tickets (wird beim Start automatisch ergänzt).
- Fehlgeschlagene Label-/Komponentensuchen werden ins Log geschrieben.

## 0.2.1 – 2026-10-31

- Zusätzliche JQL-Bedingung: Ein einzelnes Wort ohne Operator (z. B. „Tim“) wird beim Speichern
  abgewiesen, mit Hinweis auf `labels = Tim` und die Label-Zuordnung je Projekt. Schlägt die
  Abfrage mit HTTP 400 fehl, nennt die Meldung die zusätzliche Bedingung als wahrscheinliche Ursache.
- Einstellungen: „Labels je Projekt“ zeigt Farbe, Key und Namen des Projekts; das JQL-Feld erklärt
  den Unterschied zur Label-Zuordnung.

## 0.2.0 – 2026-10-31

- Favoriten erscheinen im Tray-/Menüleisten-Menü und lassen sich dort per Klick starten und
  stoppen (Häkchen = Timer läuft). Es sind höchstens 15 Favoriten möglich; beim Versuch, einen
  weiteren zu markieren, erscheint ein Hinweisdialog.
- Label-Filter je Projekt: Für Projekte mit mehreren Teams (z. B. SHARED) lassen sich in den
  Einstellungen Labels hinterlegen; es werden dann nur Tickets mit mindestens einem dieser Labels
  geladen. Die Ticketliste zeigt Labels an und bietet einen Label-Filter.
- Datenbankschema 2: Spalte `labels` an Tickets (wird beim Start automatisch ergänzt).

## 0.1.1 – 2026-10-31

- Login gegen Jira Cloud: Die Jira-URL wird beim Speichern normalisiert (Cloud: nur die Domain,
  Data Center: UI-Pfade entfernt). Mit `https://firma.atlassian.net/jira` schlug die
  Sitzungsprüfung vorher mit 404 fehl.
- Login-Fenster: Die Sitzungsprüfung läuft periodisch und zeigt ihren Stand im
  Einstellungsdialog an; bei einem 404 wird auf die falsche URL hingewiesen.
- Release-Workflow: Signatur-Variablen werden nur exportiert, wenn die Secrets gesetzt sind.
  Der macOS-Build scheiterte sonst beim Signieren.

## 0.1.0 – 2026-10-30

- Erste Version: Zeiterfassung auf Jira-Tickets (Cloud und Data Center), Browser-Login mit SSO
  und zweitem Faktor ohne API-Token, Buchung als Jira-Worklog oder über Tempo, lokale
  SQLite-Ablage mit Backups, Idle- und Shutdown-Erkennung, Statistik und Export.
