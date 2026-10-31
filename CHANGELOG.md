# Changelog

## 0.4.0 – 2026-10-31

- English language support: the UI, tray menu, notifications, dialogs and error messages are
  available in English and German. The new setting "Language" (Behavior) follows the operating
  system by default and can be forced to either language; it applies immediately.
- Dates, times and decimal hours follow the chosen language (German: `1,50`, `;`-separated CSV
  with `dd.mm.yyyy`; English: `1.50`, `,`-separated CSV with ISO dates).
- All documentation (README, plan, review, Tyme analysis, Jira auth, architecture, release notes)
  is now in English; `docs/TYME_ANALYSE.md` became `docs/TYME_ANALYSIS.md`. Screenshots show the
  English UI and the browser mock ships English sample data.
- README with screenshots and an illustrated seven-step quick guide, including the menu bar
  menu with favorites; the script `scripts/screenshots.sh` generates the images from the
  browser mock and a replica of the tray menu.
- Browser mock: `?view=`, `?tab=` and `?lang=` open a view directly in a given language.

## 0.3.0 – 2026-10-31

- Filter per project: In addition to labels, components can now be configured (the project's
  component list is offered as suggestions, spaces allowed). If both are set, you choose
  "Component and label" or "Component or label" per project. Existing label settings are migrated.
- The ticket list shows components as chips and offers a component filter.
- Database schema 3: column `components` on tickets (added automatically at startup).
- Failed label/component searches are written to the log.

## 0.2.1 – 2026-10-31

- Additional JQL condition: A single word without an operator (e.g. "Tim") is rejected on save,
  with a hint pointing to `labels = Tim` and the per-project label mapping. If the query fails
  with HTTP 400, the message names the additional condition as the likely cause.
- Settings: "Labels per project" shows the project's color, key and name; the JQL field explains
  the difference from the label mapping.

## 0.2.0 – 2026-10-31

- Favorites appear in the tray/menu bar menu and can be started and stopped there with a click
  (check mark = timer running). At most 15 favorites are allowed; trying to mark another one
  shows a notice dialog.
- Label filter per project: For projects with several teams (e.g. SHARED), labels can be
  configured in Settings; only tickets with at least one of these labels are then loaded.
  The ticket list shows labels and offers a label filter.
- Database schema 2: column `labels` on tickets (added automatically at startup).

## 0.1.1 – 2026-10-31

- Login against Jira Cloud: The Jira URL is normalized on save (Cloud: domain only,
  Data Center: UI paths removed). With `https://firma.atlassian.net/jira`, the session check
  previously failed with 404.
- Login window: The session check runs periodically and shows its status in the settings
  dialog; on a 404, the wrong URL is pointed out.
- Release workflow: Signing variables are only exported if the secrets are set. Otherwise the
  macOS build failed during signing.

## 0.1.0 – 2026-10-30

- First version: time tracking on Jira tickets (Cloud and Data Center), browser login with SSO
  and second factor without an API token, booking as a Jira worklog or via Tempo, local
  SQLite storage with backups, idle and shutdown detection, statistics and export.
