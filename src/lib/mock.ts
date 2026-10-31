// In-browser mock of the Tauri backend. Used when the app runs outside Tauri (e.g. `npm run dev`
// opened in a normal browser) so that the UI can be developed and reviewed without Jira.
import type {
  AppInfo,
  AuthStatus,
  ExportRow,
  FavoriteResult,
  IdlePrompt,
  Issue,
  IssueType,
  IssuesSnapshot,
  Project,
  RefreshStatus,
  RestorePrompt,
  Settings,
  Sprint,
  StartResult,
  Stats,
  StopResult,
  SyncSummary,
  TimeEntry,
  TimeEntryInput,
} from "./api";

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

export function mockEmit(event: string, payload: unknown = null) {
  handlers.get(event)?.forEach((h) => h(payload));
}

export function mockListen(event: string, handler: Handler): () => void {
  if (!handlers.has(event)) handlers.set(event, new Set());
  handlers.get(event)!.add(handler);
  return () => handlers.get(event)?.delete(handler);
}

const defaultSettings: Settings = {
  jira: { baseUrl: "https://jira.example.com", authMethod: "browserSession", username: "Sam Miller" },
  selection: {
    projectKeys: ["WEB", "API"],
    sprintIds: [12],
    excludedIssueTypes: ["Epic"],
    includeOpenSprints: true,
    onlyAssignedToMe: false,
    hideDone: true,
    extraJql: "",
    projectLabels: {},
  },
  refresh: { auto: true, intervalMinutes: 10 },
  timer: {
    allowConcurrent: false,
    idleTimeoutMinutes: 10,
    ignoreShortEntriesSeconds: 60,
    askNoteOnStop: false,
    bookOnStop: false,
    rounding: { minutes: 0, mode: "nearest" },
    defaultBillable: true,
    longRunningWarningHours: 10,
  },
  work: {
    hoursPerWeekday: [8, 8, 8, 8, 8, 0, 0],
    firstDayOfWeek: 0,
    timeFormat: "hoursMinutes",
    breakReminderAfterMinutes: 240,
    workdayStart: "09:30",
    dailyGoalNotification: true,
    unbookedReminder: true,
  },
  behavior: {
    closeToTray: true,
    autostart: false,
    notifications: true,
    theme: "system",
    shortcutToggleTimer: "CommandOrControl+Shift+T",
    shortcutShowWindow: "CommandOrControl+Shift+Y",
    showTimerInTray: true,
  },
  tempo: { mode: "jiraWorklog", cloudTokenSet: false, cloudApiBase: "https://api.tempo.io/4" },
  projectColors: { WEB: "#3b82f6", API: "#10b981", OPS: "#f59e0b" },
  onboardingDone: true,
};

let settings: Settings = structuredClone(defaultSettings);
let authenticated = true;

const projects: Project[] = [
  { id: "1", key: "WEB", name: "Web-Portal", avatarUrl: null, color: "#3b82f6", selected: true },
  { id: "2", key: "API", name: "Backend API", avatarUrl: null, color: "#10b981", selected: true },
  { id: "3", key: "OPS", name: "Betrieb", avatarUrl: null, color: "#f59e0b", selected: false },
];

const sprints: Sprint[] = [
  { id: 12, name: "Sprint 42", state: "active", boardId: 1, startDate: "2026-10-20T08:00:00Z", endDate: "2026-11-03T08:00:00Z", goal: "Release 3.2", selected: true },
  { id: 13, name: "Sprint 43", state: "future", boardId: 1, startDate: null, endDate: null, goal: null, selected: false },
  { id: 20, name: "API Sprint 9", state: "active", boardId: 2, startDate: "2026-10-22T08:00:00Z", endDate: "2026-11-05T08:00:00Z", goal: null, selected: false },
];

function mkIssue(p: Partial<Issue> & Pick<Issue, "key" | "summary" | "projectKey" | "issueType">): Issue {
  return {
    id: String(Math.floor(Math.random() * 100000)),
    issueTypeId: null,
    issueTypeIcon: null,
    isSubtask: p.issueType === "Sub-task",
    status: "In Arbeit",
    statusCategory: "indeterminate",
    parentKey: null,
    epicKey: null,
    sprintId: 12,
    sprintName: "Sprint 42",
    assignee: "Sam Miller",
    assigneeId: "me",
    priority: "Medium",
    dueDate: null,
    originalEstimateS: null,
    remainingEstimateS: null,
    timeSpentS: null,
    updated: new Date().toISOString(),
    labels: [],
    favorite: false,
    source: "sprint",
    lastSeen: null,
    ...p,
  };
}

let favoriteLimit = 15;

let issues: Issue[] = [
  mkIssue({ key: "WEB-101", summary: "Login-Seite: Fehlermeldung bei abgelaufener Session", projectKey: "WEB", issueType: "Bug", originalEstimateS: 7200, favorite: true, priority: "High", labels: ["team-blau", "login"] }),
  mkIssue({ key: "WEB-104", summary: "Dashboard-Widgets per Drag & Drop anordnen", projectKey: "WEB", issueType: "Story", originalEstimateS: 28800, status: "To Do", statusCategory: "new", labels: ["team-blau"] }),
  mkIssue({ key: "WEB-105", summary: "Widget-Layout speichern", projectKey: "WEB", issueType: "Sub-task", parentKey: "WEB-104", originalEstimateS: 7200 }),
  mkIssue({ key: "WEB-106", summary: "Widget-Reihenfolge laden", projectKey: "WEB", issueType: "Sub-task", parentKey: "WEB-104", originalEstimateS: 3600, status: "To Do", statusCategory: "new" }),
  mkIssue({ key: "WEB-110", summary: "Barrierefreiheit: Kontraste im Dunkelmodus prüfen", projectKey: "WEB", issueType: "Task", originalEstimateS: 10800, assignee: "Alex Weber", assigneeId: "alex", labels: ["team-rot", "a11y"] }),
  mkIssue({ key: "API-77", summary: "Rate-Limiting für /search Endpunkt", projectKey: "API", issueType: "Story", originalEstimateS: 14400, sprintId: 20, sprintName: "API Sprint 9", labels: ["backend"] }),
  mkIssue({ key: "API-80", summary: "Worklog-Webhook an Tempo weiterleiten", projectKey: "API", issueType: "Task", originalEstimateS: 5400, sprintId: 20, sprintName: "API Sprint 9", status: "Review", statusCategory: "indeterminate" }),
  mkIssue({ key: "API-81", summary: "Flaky Test: SessionRefreshIT", projectKey: "API", issueType: "Bug", sprintId: 20, sprintName: "API Sprint 9", assignee: null, assigneeId: null, status: "To Do", statusCategory: "new" }),
  mkIssue({ key: "OPS-12", summary: "Zertifikat jira.example.com erneuern", projectKey: "OPS", issueType: "Task", sprintId: null, sprintName: null, source: "tracked" }),
];

let nextId = 100;
let entries: TimeEntry[] = [];

function seedEntries() {
  const today = new Date();
  const mk = (dayOffset: number, key: string, sh: number, sm: number, eh: number, em: number, note: string, sync: TimeEntry["syncState"] = "synced"): TimeEntry => {
    const s = new Date(today.getFullYear(), today.getMonth(), today.getDate() - dayOffset, sh, sm);
    const e = new Date(today.getFullYear(), today.getMonth(), today.getDate() - dayOffset, eh, em);
    return {
      id: nextId++,
      issueKey: key,
      startedAt: s.toISOString(),
      endedAt: e.toISOString(),
      note,
      billable: true,
      syncState: sync,
      jiraWorklogId: sync === "synced" ? String(5000 + nextId) : null,
      tempoWorklogId: null,
      syncError: sync === "failed" ? "HTTP 400: Worklog darf nicht in der Zukunft liegen" : null,
      bookedSeconds: sync === "synced" ? Math.round((e.getTime() - s.getTime()) / 1000) : null,
      createdAt: s.toISOString(),
      updatedAt: e.toISOString(),
    };
  };
  entries = [
    mk(0, "WEB-101", 8, 45, 10, 15, "Session-Fehler reproduziert", "local"),
    mk(0, "WEB-105", 10, 30, 12, 0, "", "local"),
    mk(1, "WEB-101", 9, 0, 11, 30, "Analyse Token-Refresh"),
    mk(1, "API-77", 12, 30, 15, 45, "Limiter implementiert"),
    mk(1, "WEB-110", 16, 0, 17, 10, "", "failed"),
    mk(2, "API-80", 8, 30, 12, 15, "Webhook-Handler"),
    mk(2, "API-80", 13, 0, 16, 40, "Tests"),
    mk(3, "WEB-104", 9, 15, 12, 45, "Konzept DnD", "modified"),
    mk(3, "OPS-12", 14, 0, 15, 0, "Zertifikat beantragt"),
    mk(4, "WEB-101", 9, 0, 17, 30, ""),
    mk(7, "API-77", 9, 0, 16, 0, ""),
    mk(8, "WEB-104", 10, 0, 18, 0, ""),
    mk(9, "WEB-110", 9, 30, 15, 30, ""),
  ];
  const running = new Date(Date.now() - 37 * 60 * 1000);
  entries.push({
    id: nextId++,
    issueKey: "WEB-105",
    startedAt: running.toISOString(),
    endedAt: null,
    note: "Persistenz-Schicht",
    billable: true,
    syncState: "local",
    jiraWorklogId: null,
    tempoWorklogId: null,
    syncError: null,
    bookedSeconds: null,
    createdAt: running.toISOString(),
    updatedAt: running.toISOString(),
  });
}
seedEntries();

let refresh: RefreshStatus = { inProgress: false, lastSuccess: new Date(Date.now() - 4 * 60000).toISOString(), lastError: null, issueCount: issues.length };
let idlePrompt: IdlePrompt | null = null;
let restorePrompt: RestorePrompt | null = null;

const delay = (ms = 120) => new Promise((r) => setTimeout(r, ms));
const now = () => new Date().toISOString();
const dur = (e: TimeEntry) => Math.max(0, (new Date(e.endedAt ?? now()).getTime() - new Date(e.startedAt).getTime()) / 1000);

function authStatus(): AuthStatus {
  return {
    configured: !!settings.jira.baseUrl,
    authenticated,
    method: authenticated ? "cookies" : null,
    user: authenticated ? { id: "me", key: "JIRAUSER1", displayName: "Sam Miller", email: "sam@example.com", avatarUrl: null, timeZone: "Europe/Berlin" } : null,
    server: authenticated ? { baseUrl: settings.jira.baseUrl, version: "10.3.26", deployment: "dataCenter", serverTitle: "Example Jira" } : null,
    message: authenticated ? null : "Nicht angemeldet",
  };
}

function trackedSeconds(): Record<string, number> {
  const map: Record<string, number> = {};
  for (const e of entries) map[e.issueKey] = (map[e.issueKey] ?? 0) + dur(e);
  return map;
}

function stop(entry: TimeEntry, at?: string): StopResult {
  entry.endedAt = at ?? now();
  entry.updatedAt = now();
  if (dur(entry) < settings.timer.ignoreShortEntriesSeconds) {
    entries = entries.filter((e) => e.id !== entry.id);
    return { entry: null, discarded: true };
  }
  return { entry, discarded: false };
}

function dayKey(iso: string) {
  const d = new Date(iso);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function computeStats(from: string, to: string): Stats {
  const days: Stats["days"] = [];
  const fromD = new Date(from + "T00:00:00");
  const toD = new Date(to + "T00:00:00");
  const today = dayKey(now());
  for (let d = new Date(fromD); d <= toD; d.setDate(d.getDate() + 1)) {
    const key = dayKey(d.toISOString());
    const dayEntries = entries.filter((e) => dayKey(e.startedAt) === key);
    const seconds = dayEntries.reduce((a, e) => a + dur(e), 0);
    const weekday = (d.getDay() + 6) % 7;
    const sorted = [...dayEntries].sort((a, b) => a.startedAt.localeCompare(b.startedAt));
    let breaks = 0;
    for (let i = 1; i < sorted.length; i++) {
      const gap = (new Date(sorted[i].startedAt).getTime() - new Date(sorted[i - 1].endedAt ?? now()).getTime()) / 1000;
      if (gap > 0) breaks += gap;
    }
    days.push({
      date: key,
      seconds,
      billableSeconds: dayEntries.filter((e) => e.billable).reduce((a, e) => a + dur(e), 0),
      targetSeconds: key <= today ? settings.work.hoursPerWeekday[weekday] * 3600 : 0,
      breakSeconds: breaks,
      firstStart: sorted[0]?.startedAt ?? null,
      lastEnd: sorted.at(-1)?.endedAt ?? null,
      entries: dayEntries.length,
      bookedSeconds: dayEntries.filter((e) => e.syncState === "synced").reduce((a, e) => a + dur(e), 0),
    });
  }
  const inRange = entries.filter((e) => dayKey(e.startedAt) >= from && dayKey(e.startedAt) <= to);
  const byProject = new Map<string, number>();
  const byIssue = new Map<string, number>();
  for (const e of inRange) {
    const pk = issues.find((i) => i.key === e.issueKey)?.projectKey ?? e.issueKey.split("-")[0];
    byProject.set(pk, (byProject.get(pk) ?? 0) + dur(e));
    byIssue.set(e.issueKey, (byIssue.get(e.issueKey) ?? 0) + dur(e));
  }
  const total = days.reduce((a, d) => a + d.seconds, 0);
  const booked = days.reduce((a, d) => a + d.bookedSeconds, 0);
  const tracked = days.filter((d) => d.seconds > 0).length;
  return {
    from,
    to,
    totalSeconds: total,
    billableSeconds: days.reduce((a, d) => a + d.billableSeconds, 0),
    targetSeconds: days.reduce((a, d) => a + d.targetSeconds, 0),
    breakSeconds: days.reduce((a, d) => a + d.breakSeconds, 0),
    bookedSeconds: booked,
    unbookedSeconds: total - booked,
    workDays: days.filter((d) => d.targetSeconds > 0).length,
    trackedDays: tracked,
    avgPerTrackedDaySeconds: tracked ? Math.round(total / tracked) : 0,
    days,
    byProject: [...byProject.entries()]
      .map(([key, seconds]) => ({ key, label: projects.find((p) => p.key === key)?.name ?? key, seconds, projectKey: key, color: settings.projectColors[key] ?? null }))
      .sort((a, b) => b.seconds - a.seconds),
    byIssue: [...byIssue.entries()]
      .map(([key, seconds]) => {
        const i = issues.find((x) => x.key === key);
        return { key, label: i?.summary ?? "", seconds, projectKey: i?.projectKey ?? null, color: settings.projectColors[i?.projectKey ?? ""] ?? null };
      })
      .sort((a, b) => b.seconds - a.seconds),
  };
}

export const mockApi = {
  getSettings: async () => structuredClone(settings),
  saveSettings: async (s: Settings) => {
    settings = structuredClone(s);
    mockEmit("settings-changed", structuredClone(settings));
    return structuredClone(settings);
  },
  getAuthStatus: async () => authStatus(),
  startBrowserLogin: async () => {
    mockEmit("login-window", "opened");
    mockEmit("login-progress", "Warte auf Anmeldung im Fenster …");
    await delay(1500);
    mockEmit("login-window", "closed");
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  cancelBrowserLogin: async () => {},
  loginWithPassword: async (_u: string, _p: string) => {
    await delay(600);
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  loginWithToken: async () => {
    await delay(600);
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  logout: async () => {
    authenticated = false;
    mockEmit("auth-changed", authStatus());
  },
  searchProjects: async (query: string) => {
    await delay();
    const q = query.toLowerCase();
    return projects
      .filter((p) => !q || p.key.toLowerCase().includes(q) || p.name.toLowerCase().includes(q))
      .map((p) => ({ ...p, selected: settings.selection.projectKeys.includes(p.key), color: settings.projectColors[p.key] ?? p.color }));
  },
  searchSprints: async (query: string, includeClosed: boolean) => {
    await delay();
    const q = query.toLowerCase();
    return sprints
      .filter((s) => includeClosed || s.state !== "closed")
      .filter((s) => !q || s.name.toLowerCase().includes(q))
      .map((s) => ({ ...s, selected: settings.selection.sprintIds.includes(s.id) }));
  },
  listIssueTypes: async (): Promise<IssueType[]> => [
    { id: "1", name: "Bug", subtask: false, iconUrl: null },
    { id: "2", name: "Epic", subtask: false, iconUrl: null },
    { id: "3", name: "Story", subtask: false, iconUrl: null },
    { id: "4", name: "Sub-task", subtask: true, iconUrl: null },
    { id: "5", name: "Task", subtask: false, iconUrl: null },
  ],
  setProjectSelection: async (ps: Project[]) => {
    settings.selection.projectKeys = ps.map((p) => p.key);
    for (const p of ps) if (!settings.projectColors[p.key]) settings.projectColors[p.key] = p.color ?? "#64748b";
    for (const p of projects) p.selected = settings.selection.projectKeys.includes(p.key);
    mockEmit("settings-changed", structuredClone(settings));
    mockEmit("issues-updated");
    return structuredClone(settings);
  },
  setSprintSelection: async (ss: Sprint[]) => {
    settings.selection.sprintIds = ss.map((s) => s.id);
    for (const s of sprints) s.selected = settings.selection.sprintIds.includes(s.id);
    mockEmit("settings-changed", structuredClone(settings));
    return structuredClone(settings);
  },
  getIssues: async (): Promise<IssuesSnapshot> => ({
    issues: structuredClone(issues),
    projects: projects.map((p) => ({ ...p, color: settings.projectColors[p.key] ?? p.color })),
    sprints: structuredClone(sprints),
    trackedSeconds: trackedSeconds(),
    refresh: { ...refresh },
  }),
  refreshIssues: async () => {
    refresh = { ...refresh, inProgress: true };
    mockEmit("refresh-status", { ...refresh });
    await delay(900);
    refresh = { inProgress: false, lastSuccess: now(), lastError: null, issueCount: issues.length };
    mockEmit("refresh-status", { ...refresh });
    mockEmit("issues-updated");
    return { ...refresh };
  },
  getRefreshStatus: async () => ({ ...refresh }),
  /** Lets the browser test lower the limit: window.__yatttMock.__setFavoriteLimit(2) */
  __setFavoriteLimit: (n: number) => {
    favoriteLimit = n;
  },
  setFavorite: async (key: string, favorite: boolean): Promise<FavoriteResult> => {
    const limit = favoriteLimit;
    const i = issues.find((x) => x.key === key);
    const count = issues.filter((x) => x.favorite).length;
    if (favorite && i && !i.favorite && count >= limit) return { accepted: false, limit, count };
    if (i) i.favorite = favorite;
    mockEmit("issues-updated");
    return { accepted: true, limit, count: issues.filter((x) => x.favorite).length };
  },
  searchLabels: async (query: string) => {
    await delay();
    const q = query.toLowerCase();
    const all = new Set<string>(["team-blau", "team-rot", "team-gruen", "hr-core", "payroll", "backend", "login", "a11y"]);
    for (const i of issues) for (const l of i.labels) all.add(l);
    return [...all].filter((l) => !q || l.toLowerCase().includes(q)).sort();
  },
  addIssueByKey: async (key: string) => {
    await delay(400);
    const k = key.toUpperCase();
    let i = issues.find((x) => x.key === k);
    if (!i) {
      if (!/^[A-Z]+-\d+$/.test(k)) throw `Ticket ${k} wurde nicht gefunden`;
      i = mkIssue({ key: k, summary: `Manuell hinzugefügt: ${k}`, projectKey: k.split("-")[0], issueType: "Task", sprintId: null, sprintName: null, source: "manual" });
      issues.push(i);
    }
    mockEmit("issues-updated");
    return i;
  },
  setProjectColor: async (key: string, color: string | null) => {
    if (color) settings.projectColors[key] = color;
    else delete settings.projectColors[key];
    mockEmit("issues-updated");
  },
  getIssueUrl: async (key: string) => `${settings.jira.baseUrl}/browse/${key}`,
  startTimer: async (issueKey: string, note?: string, at?: string): Promise<StartResult> => {
    const stopped: TimeEntry[] = [];
    const discarded: number[] = [];
    if (!settings.timer.allowConcurrent) {
      for (const e of entries.filter((e) => !e.endedAt)) {
        const r = stop(e, at);
        if (r.entry) stopped.push(r.entry);
        else discarded.push(e.id);
      }
    }
    const started: TimeEntry = {
      id: nextId++,
      issueKey,
      startedAt: at ?? now(),
      endedAt: null,
      note: note ?? "",
      billable: settings.timer.defaultBillable,
      syncState: "local",
      jiraWorklogId: null,
      tempoWorklogId: null,
      syncError: null,
      bookedSeconds: null,
      createdAt: now(),
      updatedAt: now(),
    };
    entries.push(started);
    mockEmit("timer-changed");
    mockEmit("entries-changed");
    return { started, stopped, discarded };
  },
  stopTimer: async (entryId?: number, at?: string, note?: string): Promise<StopResult[]> => {
    const targets = entries.filter((e) => !e.endedAt && (entryId == null || e.id === entryId));
    const results = targets.map((e) => {
      if (note != null) e.note = note;
      return stop(e, at);
    });
    mockEmit("timer-changed");
    mockEmit("entries-changed");
    return results;
  },
  getRunning: async () => entries.filter((e) => !e.endedAt).map((e) => ({ ...e })),
  updateRunning: async (entryId: number, startedAt?: string, note?: string) => {
    const e = entries.find((x) => x.id === entryId)!;
    if (startedAt) e.startedAt = startedAt;
    if (note != null) e.note = note;
    mockEmit("timer-changed");
    mockEmit("entries-changed");
    return { ...e };
  },
  listEntries: async (from: string, to: string) =>
    entries.filter((e) => e.startedAt < to && (!e.endedAt || e.endedAt > from)).sort((a, b) => a.startedAt.localeCompare(b.startedAt)).map((e) => ({ ...e })),
  addEntry: async (input: TimeEntryInput) => {
    const e: TimeEntry = {
      id: nextId++,
      ...input,
      syncState: "local",
      jiraWorklogId: null,
      tempoWorklogId: null,
      syncError: null,
      bookedSeconds: null,
      createdAt: now(),
      updatedAt: now(),
    };
    entries.push(e);
    mockEmit("entries-changed");
    return { ...e };
  },
  updateEntry: async (id: number, input: TimeEntryInput) => {
    const e = entries.find((x) => x.id === id)!;
    Object.assign(e, input, { updatedAt: now(), syncState: e.syncState === "synced" ? "modified" : e.syncState });
    mockEmit("entries-changed");
    return { ...e };
  },
  deleteEntry: async (id: number) => {
    const e = entries.find((x) => x.id === id);
    if (e) (e as TimeEntry & { deleted?: boolean }).deleted = true;
    entries = entries.filter((x) => x.id !== id);
    deleted.set(id, e!);
    mockEmit("entries-changed");
  },
  restoreEntry: async (id: number) => {
    const e = deleted.get(id);
    if (e) {
      entries.push(e);
      deleted.delete(id);
    }
    mockEmit("entries-changed");
    return e ?? null;
  },
  bookEntries: async (ids: number[]): Promise<SyncSummary> => {
    await delay(700);
    const summary: SyncSummary = { booked: [], failed: [], deletedRemote: 0 };
    for (const id of ids) {
      const e = entries.find((x) => x.id === id);
      if (!e || !e.endedAt) continue;
      if (e.issueKey === "WEB-110") {
        e.syncState = "failed";
        e.syncError = "HTTP 400: Worklog darf nicht in der Zukunft liegen";
        summary.failed.push([id, e.syncError]);
      } else {
        e.syncState = "synced";
        e.jiraWorklogId = e.jiraWorklogId ?? String(7000 + id);
        e.bookedSeconds = Math.max(60, Math.round(dur(e)));
        e.syncError = null;
        summary.booked.push(id);
      }
    }
    mockEmit("entries-changed");
    mockEmit("sync-done", summary);
    return summary;
  },
  syncNow: async () => {
    const ids = entries.filter((e) => e.endedAt && (e.syncState === "pending" || e.syncState === "modified")).map((e) => e.id);
    return mockApi.bookEntries(ids);
  },
  importWorklogs: async () => {
    await delay(800);
    return 0;
  },
  getStats: async (from: string, to: string) => computeStats(from, to),
  exportEntries: async () => 0,
  getExportRows: async (from: string, to: string, summarize: boolean): Promise<ExportRow[]> => {
    const rows = entries
      .filter((e) => e.endedAt && dayKey(e.startedAt) >= from && dayKey(e.startedAt) <= to)
      .map((e) => {
        const i = issues.find((x) => x.key === e.issueKey);
        return {
          date: dayKey(e.startedAt),
          start: e.startedAt,
          end: e.endedAt!,
          durationSeconds: dur(e),
          projectKey: i?.projectKey ?? e.issueKey.split("-")[0],
          issueKey: e.issueKey,
          summary: i?.summary ?? "",
          note: e.note,
          billable: e.billable,
          syncState: e.syncState,
          jiraWorklogId: e.jiraWorklogId,
          entryIds: [e.id],
        };
      })
      .sort((a, b) => a.start.localeCompare(b.start));
    if (!summarize) return rows;
    const map = new Map<string, ExportRow>();
    for (const r of rows) {
      const k = `${r.date}|${r.issueKey}`;
      const g = map.get(k);
      if (g) {
        g.durationSeconds += r.durationSeconds;
        g.end = r.end;
        g.entryIds.push(...r.entryIds);
        if (r.note && !g.note.includes(r.note)) g.note = g.note ? `${g.note} | ${r.note}` : r.note;
      } else map.set(k, { ...r, entryIds: [...r.entryIds] });
    }
    return [...map.values()];
  },
  getIdlePrompt: async () => idlePrompt,
  respondIdle: async (decision: string) => {
    if (!idlePrompt) return;
    const since = idlePrompt.idleSince;
    for (const id of idlePrompt.entryIds) {
      const e = entries.find((x) => x.id === id);
      if (!e || e.endedAt) continue;
      if (decision === "stopAtIdleStart") stop(e, since);
      if (decision === "discardIdle") {
        stop(e, since);
        entries.push({ ...e, id: nextId++, startedAt: now(), endedAt: null, syncState: "local", jiraWorklogId: null, bookedSeconds: null });
      }
    }
    idlePrompt = null;
    mockEmit("timer-changed");
    mockEmit("entries-changed");
  },
  getRestorePrompt: async () => restorePrompt,
  respondRestore: async (entryId: number, decision: string) => {
    const e = entries.find((x) => x.id === entryId);
    if (e) {
      if (decision === "stopAtLastSeen") stop(e, restorePrompt?.lastSeen ?? now());
      if (decision === "stopNow") stop(e);
      if (decision === "discard") entries = entries.filter((x) => x.id !== entryId);
    }
    if (restorePrompt) {
      restorePrompt.entries = restorePrompt.entries.filter((x) => x.id !== entryId);
      if (!restorePrompt.entries.length) restorePrompt = null;
    }
    mockEmit("timer-changed");
    mockEmit("entries-changed");
  },
  requestQuit: async () => mockEmit("exit-requested", "quit"),
  confirmExit: async (stopTimers: boolean) => {
    if (stopTimers) for (const e of entries.filter((e) => !e.endedAt)) stop(e);
    mockEmit("timer-changed");
    mockEmit("entries-changed");
  },
  cancelExit: async () => {},
  setTempoCloudToken: async (token: string | null) => {
    settings.tempo.cloudTokenSet = !!token;
    mockEmit("settings-changed", structuredClone(settings));
  },
  getAppInfo: async (): Promise<AppInfo> => ({
    version: "0.1.0-mock",
    platform: "browser",
    configDir: "~/Library/Application Support/YaTTT (Mock)",
    dataDir: "~/Library/Application Support/YaTTT (Mock)",
    logDir: "~/Library/Application Support/YaTTT/logs (Mock)",
    secretBackend: "keyring",
  }),
  // helpers for manual UI testing in the browser
  __simulateIdle(minutes = 25) {
    const running = entries.filter((e) => !e.endedAt).map((e) => e.id);
    if (!running.length) return;
    idlePrompt = { entryIds: running, idleSince: new Date(Date.now() - minutes * 60000).toISOString(), reason: "idle" };
    mockEmit("idle-prompt", idlePrompt);
  },
  __simulateRestore() {
    const running = entries.filter((e) => !e.endedAt);
    if (!running.length) return;
    restorePrompt = { entries: running.map((e) => ({ ...e })), lastSeen: new Date(Date.now() - 15 * 60000).toISOString(), keptRunningOnExit: false };
  },
};

const deleted = new Map<number, TimeEntry>();

declare global {
  interface Window {
    __yatttMock?: typeof mockApi;
  }
}
if (typeof window !== "undefined") window.__yatttMock = mockApi;
