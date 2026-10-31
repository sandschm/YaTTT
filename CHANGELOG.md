# Änderungen

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
