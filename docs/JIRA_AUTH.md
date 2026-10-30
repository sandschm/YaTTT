# Jira-Zugriff ohne API-Token: SSO, zweiter Faktor, Testprotokoll

Dieses Dokument beschreibt, wie YaTTT sich bei Jira anmeldet, warum der gewählte Weg mit Single
Sign-on (SSO) und einem zweiten Faktor funktioniert, was bisher getestet wurde und was gegen die
echte Instanz des Teams noch zu prüfen ist.

## 1. Ausgangslage

| | Jira Cloud (`*.atlassian.net`) | Jira Data Center / Server |
|---|---|---|
| Passwort per Basic-Auth | seit 2019 abgeschaltet | möglich, sofern nicht deaktiviert; bei SSO meist nicht (Passwort liegt beim IdP) |
| Session-Login `POST /rest/auth/1/session` | nur ohne SSO; Atlassian-Konto wird über `id.atlassian.com` angemeldet | funktioniert ohne SSO; mit SAML/Crowd-SSO wird zur IdP-Seite umgeleitet, 2FA-Plugins verweigern den Login |
| Browser-Session (Cookies) | `cloud.session.token` (HttpOnly), vom Browser für alle REST-Aufrufe genutzt | `JSESSIONID` (+ `seraph.rememberme.cookie`) |
| Tokens | API-Token (Basic), OAuth 2.0 3LO | Personal Access Token (Bearer), OAuth 2.0 (ab 8.22) |

Anforderung: kein API-Token, SSO und zweiter Faktor müssen funktionieren. Damit bleibt als
universeller Weg nur die **echte Browser-Anmeldung**: Der Nutzer durchläuft den Login von Jira bzw.
des Identity Providers inklusive zweitem Faktor, danach übernimmt die App die Session-Cookies.

## 2. Umsetzung in YaTTT

1. Einstellungen → Verbindung: Jira-URL eintragen, Anmeldeart „Browser-Anmeldung (SSO, 2FA)“, „Anmelden“.
2. Die App öffnet ein eigenes Fenster (Tauri-Webview) mit `<jira-url>/login.jsp?os_destination=…`.
   Jira Cloud leitet auf `id.atlassian.com`/`auth.atlassian.com` weiter, von dort zum IdP (SAML/OIDC)
   und zur 2FA-Abfrage; Data Center leitet bei SSO ebenfalls zum IdP.
   Das Fenster meldet sich mit einem normalen Desktop-Browser-User-Agent, weil einige IdPs
   (z. B. Google) eingebettete Webviews ablehnen.
3. Alle 1,5 s liest das Backend die Cookies des Fensters für die Jira-URL
   (`Webview::cookies_for_url`, liefert auch HttpOnly-Cookies) und probiert damit
   `GET /rest/api/2/myself`. Antwort 200 → Anmeldung erfolgreich; das Fenster schließt sich.
4. Die Cookies werden verschlüsselt gespeichert (siehe Abschnitt 4). Jeder spätere REST-Aufruf
   sendet sie als `Cookie`-Header zusammen mit `X-Atlassian-Token: no-check` (XSRF-Ausnahme für
   JSON-Requests). Rotierte Cookies (`Set-Cookie` in Antworten) werden übernommen und gespeichert.
5. Antworten mit 401/403 oder einer Weiterleitung zur Login-Seite gelten als abgelaufene Sitzung.
   Die App zeigt „Erneut anmelden“; da das Login-Fenster seine Browserdaten behält, reicht beim IdP
   oft ein Klick ohne erneute Passworteingabe.

Rückfalloptionen (ebenfalls implementiert): Benutzername/Passwort (Data Center ohne SSO, geprüft
über `/rest/auth/1/session`, danach Basic-Auth), Personal Access Token (Bearer), E-Mail + API-Token
(Cloud). „Abmelden“ löscht die gespeicherten Zugangsdaten und die Browserdaten des Login-Fensters.

### Warum nicht OAuth 2.0?

OAuth 2.0 (3LO) ist der offizielle Weg für Jira Cloud ohne Token und funktioniert ebenfalls über den
(System-)Browser mit SSO/2FA. Er setzt aber eine einmalige App-Registrierung in der Atlassian
Developer Console (Client-ID/Secret) voraus, und bei Data Center eine Admin-Konfiguration
(„Incoming OAuth 2.0 Link“, Redirect-URI). Deshalb ist OAuth als zweiter Pfad vorgesehen, sobald die
Registrierung vorliegt; das Credential-Modell (`Credential`) ist dafür erweiterbar.

## 3. Was bisher getestet wurde

Ohne Zugang zur Jira-Instanz des Teams wurden folgende Punkte geprüft:

| Test | Ergebnis |
|---|---|
| Öffentliche Jira-DC-Instanz (ASF Jira, Version 10.3): `serverInfo`, `project`, `search` anonym | funktioniert, API-Formen bestätigt (`startAt`-Pagination, Sprint-Feld als String) |
| ASF Jira: `myself`, `agile/1.0/board` anonym | 401 wie erwartet; Agile-API braucht Anmeldung |
| Jira Cloud (`jira.atlassian.com`): Login-Weiterleitung | `login.jsp` → `auth.atlassian.com/authorize?…` (Atlassian-ID), REST ohne Session → 302 |
| Jira-Client gegen Mock-Server (wiremock) | Cookie- und XSRF-Header, 401/302 → „Sitzung abgelaufen“, Cookie-Rotation, 429 mit Retry-After, Cloud-Pagination (`nextPageToken`) und DC-Pagination, Worklog anlegen/Fehlermeldungen, Passwort-Login mit Set-Cookie, Kanban-Board ohne Sprints |
| Verschlüsselte Speicherung großer Cookie-Sätze | Roundtrip mit ~4,5 KB Nutzdaten, Datei enthält keinen Klartext |
| Native App (macOS, Debug-Build): Login-Fenster gegen ASF Jira | Fenster öffnet die echte Jira-Loginseite; das Backend liest `JSESSIONID` und `atlassian.xsrf.token` (beide HttpOnly) aus der Webview und prüft sie gegen `/rest/api/2/myself` (anonym 401 → wartet weiter). Damit ist der Mechanismus bestätigt, auf dem der SSO/2FA-Login beruht. |

Nicht testbar ohne die echte Instanz: der tatsächliche SSO-/2FA-Durchlauf im Login-Fenster und
die Gültigkeitsdauer der Session.

## 4. Speicherung der Zugangsdaten

- Im Schlüsselbund des Betriebssystems (macOS Keychain, Windows Credential Manager, Linux Secret
  Service) liegt nur ein zufälliger 32-Byte-Schlüssel (Service `de.yattt.app`).
- Session-Cookies, Tokens und ggf. das Passwort liegen XChaCha20-Poly1305-verschlüsselt in
  `secrets.enc` im Konfigurationsverzeichnis (macOS: `~/Library/Application Support/YaTTT`,
  Windows: `%APPDATA%\YaTTT`, Linux: `~/.config/YaTTT`).
- Ohne Schlüsselbund (z. B. Linux ohne Secret Service) liegt der Schlüssel in `secrets.key`
  mit Rechten 0600; die Einstellungen zeigen dann einen Hinweis.
- Bei SSO wird nie ein Passwort gespeichert, nur die Sitzung. Das weicht bewusst von der wörtlichen
  Anforderung „Login und Passwort speichern“ ab, weil die App das Passwort bei SSO nie sieht.

## 5. Checkliste für den Test gegen die echte Instanz

1. Jira-URL eintragen, „Anmelden“: Erscheint die bekannte Login-Seite bzw. die IdP-Seite?
2. SSO und zweiter Faktor durchlaufen. Schließt sich das Fenster und erscheint der Benutzername
   unter „Status“? Falls nicht: Logdatei (`<Datenverzeichnis>/logs/yattt.log`) prüfen, insbesondere
   Zeilen mit `probe failed` oder `cookies_for_url`.
3. Tickets laden (Projekt wählen). Erscheinen die Tickets des aktiven Sprints?
4. Timer starten/stoppen und buchen. Erscheint der Worklog im Ticket bzw. in Tempo?
   Fehlertexte stehen am Eintrag (Badge „Fehler“).
5. App beenden und neu starten: Bleibt die Anmeldung erhalten?
6. Nach einem Arbeitstag: Ist die Sitzung noch gültig? Wenn Jira/Guard kurze Sitzungen erzwingt,
   lohnt sich OAuth 2.0 (Refresh-Token) als nächster Schritt.
7. Falls der IdP das eingebettete Fenster ablehnt (Meldung wie „Dieser Browser wird nicht
   unterstützt“): Meldung notieren; dann ist OAuth 2.0 über den Systembrowser der nächste Schritt.

## 6. Erster Test gegen die echte Instanz (30.10.2026)

Jira Cloud (`acme.atlassian.net`): Der SSO-Login im Fenster hat funktioniert, die App las
`tenant.session.token`, `atlassian.account.xsrf.token` und weitere Cookies aus. Die Sitzungsprüfung
schlug trotzdem fehl, weil die Jira-URL mit dem UI-Pfad `/jira` eingetragen war; die REST-API von
Jira Cloud liegt immer unter der Domain-Wurzel. Behoben: URLs werden beim Speichern normalisiert
(Cloud: nur Origin; Data Center: UI-Pfade wie `/browse/…` entfernt, Kontextpfad bleibt), die
Prüfung läuft periodisch und meldet ihren Stand im Einstellungsdialog.

## 7. Bekannte Einschränkungen

- Atlassian bezeichnet Cookie-Authentifizierung für externe Clients als veraltet. Der Browser
  selbst arbeitet damit, eine Garantie gibt es dafür aber nicht.
- Atlassian Guard kann Sitzungsdauern begrenzen (Idle-Timeout ab 15 Minuten, absolute Dauer).
- Nach mehreren Fehlversuchen verlangt Jira ein CAPTCHA; die Passwort-Anmeldung meldet das.
- Unternehmens-Proxys und interne Zertifikate: der Rust-Client nutzt den System-Zertifikatspeicher
  und die Proxy-Umgebungsvariablen (`HTTPS_PROXY`).
