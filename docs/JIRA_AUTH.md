# Jira access without an API token: SSO, second factor, test log

This document describes how YaTTT signs in to Jira, why the chosen approach works with single
sign-on (SSO) and a second factor, what has been tested so far, and what still needs to be checked
against the team's real instance.

## 1. Starting point

| | Jira Cloud (`*.atlassian.net`) | Jira Data Center / Server |
|---|---|---|
| Password via Basic Auth | disabled since 2019 | possible unless deactivated; usually not with SSO (the password lives at the IdP) |
| Session login `POST /rest/auth/1/session` | only without SSO; the Atlassian account is signed in via `id.atlassian.com` | works without SSO; with SAML/Crowd SSO it redirects to the IdP page, 2FA plugins refuse the login |
| Browser session (cookies) | `cloud.session.token` (HttpOnly), used by the browser for all REST calls | `JSESSIONID` (+ `seraph.rememberme.cookie`) |
| Tokens | API token (Basic), OAuth 2.0 3LO | Personal Access Token (Bearer), OAuth 2.0 (from 8.22) |

Requirement: no API token; SSO and a second factor must work. That leaves the **real browser
login** as the only universal approach: the user goes through the login of Jira or of the identity
provider, including the second factor, and the app then takes over the session cookies.

## 2. Implementation in YaTTT

1. Settings → Connection: enter the Jira URL, choose the login type "Browser login (SSO, 2FA)", click "Sign in".
2. The app opens its own window (Tauri webview) with `<jira-url>/login.jsp?os_destination=…`.
   Jira Cloud redirects to `id.atlassian.com`/`auth.atlassian.com`, from there to the IdP (SAML/OIDC)
   and to the 2FA prompt; Data Center with SSO also redirects to the IdP.
   The window identifies itself with a normal desktop browser user agent, because some IdPs
   (e.g. Google) reject embedded webviews.
3. Every 1.5 s, the backend reads the window's cookies for the Jira URL
   (`Webview::cookies_for_url`, which also returns HttpOnly cookies) and tries
   `GET /rest/api/2/myself` with them. Response 200 → login successful; the window closes.
4. The cookies are stored encrypted (see section 4). Every later REST call sends them as a
   `Cookie` header together with `X-Atlassian-Token: no-check` (XSRF exemption for JSON
   requests). Rotated cookies (`Set-Cookie` in responses) are taken over and stored.
5. Responses with 401/403 or a redirect to the login page count as an expired session.
   The app shows "Sign in again"; since the login window keeps its browser data, a single click
   at the IdP is often enough, without entering the password again.

Fallback options (also implemented): username/password (Data Center without SSO, verified via
`/rest/auth/1/session`, then Basic Auth), Personal Access Token (Bearer), email + API token
(Cloud). "Sign out" deletes the stored credentials and the login window's browser data.

### Why not OAuth 2.0?

OAuth 2.0 (3LO) is the official approach for Jira Cloud without a token and also works via the
(system) browser with SSO/2FA. However, it requires a one-time app registration in the Atlassian
Developer Console (client ID/secret), and on Data Center an admin configuration
("Incoming OAuth 2.0 Link", redirect URI). OAuth is therefore planned as a second path once the
registration is available; the credential model (`Credential`) can be extended for it.

## 3. What has been tested so far

Without access to the team's Jira instance, the following points were checked:

| Test | Result |
|---|---|
| Public Jira DC instance (ASF Jira, version 10.3): `serverInfo`, `project`, `search` anonymously | works, API shapes confirmed (`startAt` pagination, sprint field as string) |
| ASF Jira: `myself`, `agile/1.0/board` anonymously | 401 as expected; the Agile API requires login |
| Jira Cloud (`jira.atlassian.com`): login redirect | `login.jsp` → `auth.atlassian.com/authorize?…` (Atlassian ID), REST without session → 302 |
| Jira client against a mock server (wiremock) | Cookie and XSRF headers, 401/302 → "session expired", cookie rotation, 429 with Retry-After, Cloud pagination (`nextPageToken`) and DC pagination, creating worklogs/error messages, password login with Set-Cookie, Kanban board without sprints |
| Encrypted storage of large cookie sets | Round trip with ~4.5 KB of payload, file contains no plaintext |
| Native app (macOS, debug build): login window against ASF Jira | The window opens the real Jira login page; the backend reads `JSESSIONID` and `atlassian.xsrf.token` (both HttpOnly) from the webview and checks them against `/rest/api/2/myself` (anonymous 401 → keeps waiting). This confirms the mechanism the SSO/2FA login relies on. |

Not testable without the real instance: the actual SSO/2FA flow in the login window and the
session lifetime.

## 4. Credential storage

- The OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service) holds only
  a random 32-byte key (service `de.yattt.app`).
- Session cookies, tokens and, if applicable, the password are stored XChaCha20-Poly1305-encrypted
  in `secrets.enc` in the config directory (macOS: `~/Library/Application Support/YaTTT`,
  Windows: `%APPDATA%\YaTTT`, Linux: `~/.config/YaTTT`).
- Without a keychain (e.g. Linux without Secret Service), the key is stored in `secrets.key`
  with permissions 0600; Settings then shows a notice.
- With SSO, a password is never stored, only the session. This deliberately deviates from the
  literal requirement "store login and password", because with SSO the app never sees the password.

## 5. Checklist for testing against the real instance

1. Enter the Jira URL, click "Sign in": does the familiar login page or the IdP page appear?
2. Go through SSO and the second factor. Does the window close and does the username appear
   under "Status"? If not: check the log file (`<data directory>/logs/yattt.log`), especially
   lines containing `probe failed` or `cookies_for_url`.
3. Load tickets (select a project). Do the tickets of the active sprint appear?
4. Start/stop a timer and book it. Does the worklog appear in the ticket or in Tempo?
   Error messages are shown on the entry (badge "Error").
5. Quit and restart the app: is the login retained?
6. After a working day: is the session still valid? If Jira/Guard enforces short sessions,
   OAuth 2.0 (refresh token) is worth pursuing as the next step.
7. If the IdP rejects the embedded window (a message like "This browser is not
   supported"): note the message; OAuth 2.0 via the system browser is then the next step.

## 6. First test against the real instance (30.10.2026)

Jira Cloud (`acme.atlassian.net`): the SSO login in the window worked; the app read
`tenant.session.token`, `atlassian.account.xsrf.token` and other cookies. The session check
still failed because the Jira URL had been entered with the UI path `/jira`; the Jira Cloud REST
API always lives at the domain root. Fixed: URLs are normalized when saved (Cloud: origin only;
Data Center: UI paths such as `/browse/…` are removed, the context path is kept), the check runs
periodically and reports its state in the Settings dialog.

**Result (31.10.2026):** With the corrected version (0.1.1), login via SSO and a second factor
was confirmed against the team instance: session detected, user loaded, ticket list populated.
The browser-session approach without an API token is therefore ready for production use.

## 7. Known limitations

- Atlassian describes cookie authentication for external clients as deprecated. The browser
  itself works with it, but there is no guarantee.
- Atlassian Guard can limit session durations (idle timeout from 15 minutes, absolute duration).
- After several failed attempts, Jira requires a CAPTCHA; the password login reports this.
- Corporate proxies and internal certificates: the Rust client uses the system certificate store
  and the proxy environment variables (`HTTPS_PROXY`).
