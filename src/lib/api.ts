// Typed wrappers around the Tauri commands. Types mirror the Rust structs (serde camelCase).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Deployment = "unknown" | "cloud" | "dataCenter";
export type AuthMethod = "browserSession" | "password" | "personalAccessToken" | "apiToken";
export type RoundingMode = "nearest" | "up" | "down";
export type TimeFormat = "hoursMinutes" | "decimal";
export type Theme = "system" | "light" | "dark";
export type TempoMode = "jiraWorklog" | "tempoDataCenter" | "tempoCloud";
export type SyncState = "local" | "pending" | "synced" | "modified" | "failed";
export type IdleDecision = "keep" | "stopAtIdleStart" | "discardIdle";
export type RestoreDecision = "keepRunning" | "stopAtLastSeen" | "stopNow" | "discard";

export interface Settings {
  jira: { baseUrl: string; authMethod: AuthMethod; username: string | null };
  selection: {
    projectKeys: string[];
    sprintIds: number[];
    excludedIssueTypes: string[];
    includeOpenSprints: boolean;
    onlyAssignedToMe: boolean;
    hideDone: boolean;
    extraJql: string;
  };
  refresh: { auto: boolean; intervalMinutes: number };
  timer: {
    allowConcurrent: boolean;
    idleTimeoutMinutes: number;
    ignoreShortEntriesSeconds: number;
    askNoteOnStop: boolean;
    bookOnStop: boolean;
    rounding: { minutes: number; mode: RoundingMode };
    defaultBillable: boolean;
    longRunningWarningHours: number;
  };
  work: {
    hoursPerWeekday: number[];
    firstDayOfWeek: number;
    timeFormat: TimeFormat;
    breakReminderAfterMinutes: number;
    workdayStart: string;
    dailyGoalNotification: boolean;
    unbookedReminder: boolean;
  };
  behavior: {
    closeToTray: boolean;
    autostart: boolean;
    notifications: boolean;
    theme: Theme;
    shortcutToggleTimer: string;
    shortcutShowWindow: string;
    showTimerInTray: boolean;
  };
  tempo: { mode: TempoMode; cloudTokenSet: boolean; cloudApiBase: string };
  projectColors: Record<string, string>;
  onboardingDone: boolean;
}

export interface ServerInfo {
  baseUrl: string;
  version: string;
  deployment: Deployment;
  serverTitle: string | null;
}

export interface JiraUser {
  id: string;
  key: string | null;
  displayName: string;
  email: string | null;
  avatarUrl: string | null;
  timeZone: string | null;
}

export interface AuthStatus {
  configured: boolean;
  authenticated: boolean;
  method: string | null;
  user: JiraUser | null;
  server: ServerInfo | null;
  message: string | null;
}

export interface Project {
  id: string;
  key: string;
  name: string;
  avatarUrl: string | null;
  color: string | null;
  selected: boolean;
}

export interface Sprint {
  id: number;
  name: string;
  state: string;
  boardId: number | null;
  startDate: string | null;
  endDate: string | null;
  goal: string | null;
  selected: boolean;
}

export interface IssueType {
  id: string;
  name: string;
  subtask: boolean;
  iconUrl: string | null;
}

export interface Issue {
  id: string;
  key: string;
  projectKey: string;
  summary: string;
  issueType: string;
  issueTypeId: string | null;
  issueTypeIcon: string | null;
  isSubtask: boolean;
  status: string;
  statusCategory: string;
  parentKey: string | null;
  epicKey: string | null;
  sprintId: number | null;
  sprintName: string | null;
  assignee: string | null;
  assigneeId: string | null;
  priority: string | null;
  dueDate: string | null;
  originalEstimateS: number | null;
  remainingEstimateS: number | null;
  timeSpentS: number | null;
  updated: string | null;
  favorite: boolean;
  source: string;
  lastSeen: string | null;
}

export interface TimeEntry {
  id: number;
  issueKey: string;
  startedAt: string;
  endedAt: string | null;
  note: string;
  billable: boolean;
  syncState: SyncState;
  jiraWorklogId: string | null;
  tempoWorklogId: string | null;
  syncError: string | null;
  bookedSeconds: number | null;
  createdAt: string;
  updatedAt: string;
}

export interface TimeEntryInput {
  issueKey: string;
  startedAt: string;
  endedAt: string | null;
  note: string;
  billable: boolean;
}

export interface RefreshStatus {
  inProgress: boolean;
  lastSuccess: string | null;
  lastError: string | null;
  issueCount: number;
}

export interface IssuesSnapshot {
  issues: Issue[];
  projects: Project[];
  sprints: Sprint[];
  trackedSeconds: Record<string, number>;
  refresh: RefreshStatus;
}

export interface StartResult {
  started: TimeEntry;
  stopped: TimeEntry[];
  discarded: number[];
}

export interface StopResult {
  entry: TimeEntry | null;
  discarded: boolean;
}

export interface SyncSummary {
  booked: number[];
  failed: [number, string][];
  deletedRemote: number;
}

export interface DayStat {
  date: string;
  seconds: number;
  billableSeconds: number;
  targetSeconds: number;
  breakSeconds: number;
  firstStart: string | null;
  lastEnd: string | null;
  entries: number;
  bookedSeconds: number;
}

export interface GroupStat {
  key: string;
  label: string;
  seconds: number;
  projectKey: string | null;
  color: string | null;
}

export interface Stats {
  from: string;
  to: string;
  totalSeconds: number;
  billableSeconds: number;
  targetSeconds: number;
  breakSeconds: number;
  bookedSeconds: number;
  unbookedSeconds: number;
  workDays: number;
  trackedDays: number;
  avgPerTrackedDaySeconds: number;
  days: DayStat[];
  byProject: GroupStat[];
  byIssue: GroupStat[];
}

export interface ExportRow {
  date: string;
  start: string;
  end: string;
  durationSeconds: number;
  projectKey: string;
  issueKey: string;
  summary: string;
  note: string;
  billable: boolean;
  syncState: string;
  jiraWorklogId: string | null;
  entryIds: number[];
}

export interface IdlePrompt {
  entryIds: number[];
  idleSince: string;
  reason: "idle" | "sleep";
}

export interface RestorePrompt {
  entries: TimeEntry[];
  lastSeen: string | null;
  keptRunningOnExit: boolean;
}

export interface AppInfo {
  version: string;
  platform: string;
  configDir: string;
  dataDir: string;
  logDir: string;
  secretBackend: "keyring" | "file";
}

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauriApi = {
  // settings
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  // auth
  getAuthStatus: () => invoke<AuthStatus>("get_auth_status"),
  startBrowserLogin: () => invoke<AuthStatus>("start_browser_login"),
  cancelBrowserLogin: () => invoke<void>("cancel_browser_login"),
  loginWithPassword: (username: string, password: string) => invoke<AuthStatus>("login_with_password", { username, password }),
  loginWithToken: (kind: "pat" | "apiToken", user: string | null, token: string) =>
    invoke<AuthStatus>("login_with_token", { kind, user, token }),
  logout: () => invoke<void>("logout"),
  // jira metadata
  searchProjects: (query: string) => invoke<Project[]>("search_projects", { query }),
  searchSprints: (query: string, includeClosed: boolean) => invoke<Sprint[]>("search_sprints", { query, includeClosed }),
  listIssueTypes: () => invoke<IssueType[]>("list_issue_types"),
  setProjectSelection: (projects: Project[]) => invoke<Settings>("set_project_selection", { projects }),
  setSprintSelection: (sprints: Sprint[]) => invoke<Settings>("set_sprint_selection", { sprints }),
  // issues
  getIssues: () => invoke<IssuesSnapshot>("get_issues"),
  refreshIssues: () => invoke<RefreshStatus>("refresh_issues"),
  getRefreshStatus: () => invoke<RefreshStatus>("get_refresh_status"),
  setFavorite: (key: string, favorite: boolean) => invoke<void>("set_favorite", { key, favorite }),
  addIssueByKey: (key: string) => invoke<Issue>("add_issue_by_key", { key }),
  setProjectColor: (key: string, color: string | null) => invoke<void>("set_project_color", { key, color }),
  getIssueUrl: (key: string) => invoke<string>("get_issue_url", { key }),
  // timer
  startTimer: (issueKey: string, note?: string, at?: string) => invoke<StartResult>("start_timer", { issueKey, note: note ?? null, at: at ?? null }),
  stopTimer: (entryId?: number, at?: string, note?: string) =>
    invoke<StopResult[]>("stop_timer", { entryId: entryId ?? null, at: at ?? null, note: note ?? null }),
  getRunning: () => invoke<TimeEntry[]>("get_running"),
  updateRunning: (entryId: number, startedAt?: string, note?: string) =>
    invoke<TimeEntry>("update_running", { entryId, startedAt: startedAt ?? null, note: note ?? null }),
  // entries
  listEntries: (from: string, to: string) => invoke<TimeEntry[]>("list_entries", { from, to }),
  addEntry: (input: TimeEntryInput) => invoke<TimeEntry>("add_entry", { input }),
  updateEntry: (id: number, input: TimeEntryInput) => invoke<TimeEntry>("update_entry", { id, input }),
  deleteEntry: (id: number) => invoke<void>("delete_entry", { id }),
  restoreEntry: (id: number) => invoke<TimeEntry | null>("restore_entry", { id }),
  // sync
  bookEntries: (ids: number[]) => invoke<SyncSummary>("book_entries", { ids }),
  syncNow: () => invoke<SyncSummary>("sync_now"),
  importWorklogs: (from: string, to: string) => invoke<number>("import_worklogs", { from, to }),
  // stats / export
  getStats: (from: string, to: string) => invoke<Stats>("get_stats", { from, to }),
  exportEntries: (from: string, to: string, format: "csv" | "json", summarize: boolean, path: string) =>
    invoke<number>("export_entries", { from, to, format, summarize, path }),
  getExportRows: (from: string, to: string, summarize: boolean) => invoke<ExportRow[]>("get_export_rows", { from, to, summarize }),
  // idle / restore / exit
  getIdlePrompt: () => invoke<IdlePrompt | null>("get_idle_prompt"),
  respondIdle: (decision: IdleDecision) => invoke<void>("respond_idle", { decision }),
  getRestorePrompt: () => invoke<RestorePrompt | null>("get_restore_prompt"),
  respondRestore: (entryId: number, decision: RestoreDecision) => invoke<void>("respond_restore", { entryId, decision }),
  requestQuit: () => invoke<void>("request_quit"),
  confirmExit: (stopTimers: boolean) => invoke<void>("confirm_exit", { stopTimers }),
  cancelExit: () => invoke<void>("cancel_exit"),
  // tempo
  setTempoCloudToken: (token: string | null) => invoke<void>("set_tempo_cloud_token", { token }),
  // info
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
};

// Outside Tauri (plain browser during development) a mock backend is used so the UI can be
// exercised without Jira. The mock lives in ./mock.ts and is loaded lazily.
import { mockApi, mockListen } from "./mock";

export const api: typeof tauriApi = isTauri ? tauriApi : (mockApi as unknown as typeof tauriApi);

export type AppEvent =
  | "issues-updated"
  | "timer-changed"
  | "entries-changed"
  | "auth-changed"
  | "refresh-status"
  | "idle-prompt"
  | "exit-requested"
  | "settings-changed"
  | "sync-done"
  | "login-window";

export function on<T = unknown>(event: AppEvent, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri) {
    const off = mockListen(event, (p) => handler(p as T));
    return Promise.resolve(off);
  }
  return listen<T>(event, (e) => handler(e.payload));
}
