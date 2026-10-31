# Tyme analysis: feature set and how it maps to YaTTT

Source: https://www.tyme-app.com/de/ and https://www.tyme-app.com/de/all-features/ (as of 30.10.2026).

Tyme is a time tracking tool for Apple platforms (Mac, iPhone, iPad, Watch, web beta).
YaTTT ("Yet another Time Tracking Tool") adopts the part of the feature set that makes sense
for a desktop app with Jira as the only source of projects and tasks.

Legend for the "YaTTT" column:

- **Yes** – implemented in v1
- **Adapted** – implemented, but adapted to Jira
- **Later** – useful, but not part of v1
- **No** – does not fit the concept (Jira as source, single-user desktop app)

## 1. Time tracking & timers

| Tyme | YaTTT | Implementation in YaTTT |
|---|---|---|
| Start/stop timer per task | Yes | Timer per Jira ticket; start/stop in the ticket list, in the tray and via hotkey |
| Multiple simultaneous timers (setting) | Yes | Setting "Allow multiple timers"; default: one timer, starting a new one stops the old one |
| Running timers always visible | Yes | Timer bar at the top of the window, tray title (macOS) with running time |
| Adjust start time afterwards, backdate, "stop as if work had ended earlier" | Yes | Running timer: start time editable, "Stop at …" |
| Manual time entries | Yes | Entry with ticket, start, end, note |
| Slot vs. cluster mode | Adapted | Entries are always stored as slots (start/end); the "Summarize per day and ticket" view provides the cluster view and is the basis for booking to Jira/Tempo |
| Automatic timeout on inactivity (idle) | Yes | Idle detection (configurable threshold); on return, a prompt: keep time / subtract idle time / stop timer at idle start |
| Behavior on sleep/absence | Yes | Sleep detection via time-jump watchdog, same prompt as for idle |
| Record breaks automatically | Adapted | Breaks result from gaps between entries and are shown in the timesheet/statistics |
| Notes on start/stop (dialog, optional) | Yes | Setting "Ask for note when stopping"; the note becomes the worklog comment |
| Ignore short entries | Yes | Setting "Discard entries under N seconds" |
| Rounding (per project/task) | Adapted | Global rounding rule (none/5/6/10/15/30 min, up/down/nearest) for booking to Jira; local times stay exact |
| Geofencing / work location | No | Desktop app without location data |
| Travel costs, expenses, fixed costs, flat rates | No | No billing system; Jira/Tempo is the target |
| Global keyboard shortcuts | Yes | Start/stop last timer, show/hide window, quick start |

## 2. Projects, tasks, budgets

| Tyme | YaTTT | Implementation |
|---|---|---|
| Projects, tasks, subtasks, categories | Adapted | Jira project → project, Jira ticket → task, Jira subtask → subtask, epic → grouping (can optionally be hidden) |
| Colors per project (8 presets + custom) | Yes | Color per Jira project selectable locally, default from palette |
| Start/due date, deadline hints | Adapted | Show due date from Jira, sprint end as deadline |
| Planned time / budget, target/actual bar | Adapted | Original estimate / remaining from Jira time tracking as budget; bar "tracked vs. estimated" |
| Hourly rates, revenue, currency | No | Not the job of a Jira tracker |
| Billable / non-billable | Adapted | "Billable" flag per entry (relevant for Tempo), default per project |
| Archive | Adapted | Removing a project from the selection = archiving; completed tickets are hidden (filter) |
| Team features | No | Jira handles team management |

## 3. Search & filters

| Tyme | YaTTT | Implementation |
|---|---|---|
| Global search across projects/tasks/notes | Yes | Search in the ticket list (key, title), notes in time entries |
| Filter active/archived, task type, billable | Adapted | Filters: sprint, ticket type, status, "assigned to me only", "with times only" |
| Date range filter | Yes | Date range in the Times and Statistics views |
| Search when selecting projects/sprints | Yes | Search fields in Settings (requirement) |

## 4. Reports, export, statistics

| Tyme | YaTTT | Implementation |
|---|---|---|
| CSV, JSON, PDF export | Yes | CSV and JSON natively; PDF via the print view (timesheet) with the system dialog "Save as PDF" |
| Timesheet with breaks and overtime | Yes | Print view per day/period with working time, breaks, target/actual |
| Summarize: one line per task and day | Yes | Option on export |
| Accounting exports (Lexware, sevdesk …) | No | The target is Jira/Tempo |
| Statistics: hours per day, distribution per project, heatmap, overtime/undertime, breaks, average | Yes | Statistics view with bars (day), distribution (project/ticket), heatmap (year), key figures against target working hours |
| Team utilization | No | – |

## 5. Calendar, reminders, menu bar

| Tyme | YaTTT | Implementation |
|---|---|---|
| Calendar sync (bidirectional) | Later | v1: weekly timeline in the app (entries as blocks, movable and editable) |
| Reminders: timer not started, daily goal reached, break, unbooked times | Yes | Desktop notifications: no timer after start of work, daily goal reached, break reminder after N hours, unbooked times exist, timer running unusually long |
| Menu bar: running time, daily total, current task | Yes | Tray icon with menu (recent tickets, stop, window, quit); title with time on macOS |
| Widgets, Apple Watch, Siri, Spotlight, Shortcuts | No | Platform-specific |
| URL scheme / automation | Later | `yattt://start/KEY` is prepared, not in v1 |
| MCP server, plugin API, AppleScript | Later | – |

## 6. Data, sync, settings

| Tyme | YaTTT | Implementation |
|---|---|---|
| iCloud/team sync | Adapted | Jira/Tempo is the server. Locally: SQLite as a safe copy of all times (requirement "store locally to be safe") |
| Import (Jira, CSV …) | Adapted | Import the user's existing worklogs from Jira (date range) so that target/actual is correct |
| Backup & undo | Adapted | Daily backup of the SQLite file (rotating); undo for deleting an entry |
| Light/dark | Yes | System color scheme, switchable |
| Start at login | Yes | Autostart plugin |
| Time format (h:mm, decimal) | Yes | Setting |
| Working hours (flexible per weekday), first day of week | Yes | Target hours per weekday, week start for statistics |

## 7. Jira-specific features (not in Tyme)

- Connection: Jira URL, login via an embedded browser window (SSO, 2FA) or username/password, PAT or API token as fallback options
- Selection of projects and sprints with search; tickets of the active sprint of all selected projects are always loaded
- Filter for ticket types (e.g. hide epics)
- Automatic refresh of the ticket list (configurable interval) and manual refresh
- Booking times as worklogs to Jira (Tempo picks them up automatically) or directly via the Tempo API; status per entry (local / pending / booked / error), retry on errors
- Open ticket in the browser
- Prompt on shutdown/restart/quit with a running timer; restore on next start
