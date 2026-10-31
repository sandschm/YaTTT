// Global reactive state (Svelte 5 runes). One instance shared by all views.
import { api, on, type AuthStatus, type FavoriteResult, type IdlePrompt, type Issue, type IssuesSnapshot, type RestorePrompt, type Settings, type TimeEntry } from "./api";

export type View = "tickets" | "times" | "stats" | "settings";

export interface Toast {
  id: number;
  message: string;
  kind: "info" | "success" | "error";
  action?: { label: string; run: () => void | Promise<void> };
}

class AppStore {
  view = $state<View>("tickets");
  settings = $state<Settings | null>(null);
  auth = $state<AuthStatus>({ configured: false, authenticated: false, method: null, user: null, server: null, message: null });
  snapshot = $state<IssuesSnapshot | null>(null);
  running = $state<TimeEntry[]>([]);
  now = $state(Date.now());
  idlePrompt = $state<IdlePrompt | null>(null);
  restorePrompt = $state<RestorePrompt | null>(null);
  exitRequested = $state<null | "quit" | "shutdown">(null);
  /** Set when marking a favorite was refused because the limit is reached. */
  favoriteLimit = $state<FavoriteResult | null>(null);
  toasts = $state<Toast[]>([]);
  entriesVersion = $state(0);
  loginWindowOpen = $state(false);
  loginProgress = $state("");
  ready = $state(false);
  autoSwitched = false;

  private toastId = 1;
  private unlisteners: Array<() => void> = [];

  issues = $derived(this.snapshot?.issues ?? []);
  projects = $derived(this.snapshot?.projects ?? []);
  issueMap = $derived(new Map(this.issues.map((i) => [i.key, i])));
  projectMap = $derived(new Map(this.projects.map((p) => [p.key, p])));
  timeFormat = $derived(this.settings?.work.timeFormat ?? "hoursMinutes");
  connected = $derived(this.auth.authenticated);

  async init() {
    const [settings, auth, snapshot, running, idle, restore] = await Promise.all([
      api.getSettings(),
      api.getAuthStatus(),
      api.getIssues(),
      api.getRunning(),
      api.getIdlePrompt(),
      api.getRestorePrompt(),
    ]);
    this.settings = settings;
    this.auth = auth;
    this.snapshot = snapshot;
    this.running = running;
    this.idlePrompt = idle;
    this.restorePrompt = restore;
    if (!auth.configured || !auth.authenticated) {
      this.view = "settings";
      this.autoSwitched = true;
    }

    this.unlisteners.push(
      await on("issues-updated", () => this.reloadIssues()),
      await on("timer-changed", () => this.reloadRunning()),
      await on("entries-changed", () => {
        this.entriesVersion++;
        this.reloadIssues();
      }),
      await on<AuthStatus>("auth-changed", (a) => {
        this.auth = a;
        if (a.message && !a.authenticated) this.toast(a.message, "error");
        // The stored session is restored asynchronously after start; jump back to the tickets
        // once it is there, unless the user navigated to the settings on purpose.
        if (a.authenticated && this.autoSwitched && this.view === "settings") {
          this.view = "tickets";
          this.autoSwitched = false;
        }
      }),
      await on<IssuesSnapshot["refresh"]>("refresh-status", (r) => {
        if (this.snapshot) this.snapshot = { ...this.snapshot, refresh: r };
        if (r.lastError) this.toast(`Aktualisierung fehlgeschlagen: ${r.lastError}`, "error");
      }),
      await on<IdlePrompt>("idle-prompt", (p) => (this.idlePrompt = p)),
      await on<"quit" | "shutdown">("exit-requested", (reason) => (this.exitRequested = reason ?? "quit")),
      await on<Settings>("settings-changed", (s) => (this.settings = s)),
      await on<string>("login-window", (s) => {
        this.loginWindowOpen = s === "opened";
        if (!this.loginWindowOpen) this.loginProgress = "";
      }),
      await on<string>("login-progress", (s) => (this.loginProgress = s)),
      await on<{ booked: number[]; failed: [number, string][] }>("sync-done", (s) => {
        if (s.booked.length) this.toast(`${s.booked.length} ${s.booked.length === 1 ? "Eintrag" : "Einträge"} nach Jira gebucht`, "success");
        if (s.failed.length) this.toast(`${s.failed.length} Buchung(en) fehlgeschlagen`, "error");
      }),
    );
    setInterval(() => (this.now = Date.now()), 1000);
    this.ready = true;
  }

  async reloadIssues() {
    this.snapshot = await api.getIssues();
  }

  async reloadRunning() {
    this.running = await api.getRunning();
  }

  issue(key: string): Issue | undefined {
    return this.issueMap.get(key);
  }

  color(projectKey: string): string {
    return this.settings?.projectColors[projectKey] ?? this.projectMap.get(projectKey)?.color ?? "#64748b";
  }

  runningFor(key: string): TimeEntry | undefined {
    return this.running.find((e) => e.issueKey === key);
  }

  elapsed(entry: TimeEntry): number {
    const end = entry.endedAt ? new Date(entry.endedAt).getTime() : this.now;
    return Math.max(0, Math.floor((end - new Date(entry.startedAt).getTime()) / 1000));
  }

  toast(message: string, kind: Toast["kind"] = "info", action?: Toast["action"], timeoutMs = 5000) {
    const id = this.toastId++;
    this.toasts = [...this.toasts, { id, message, kind, action }];
    setTimeout(() => this.dismissToast(id), action ? 8000 : timeoutMs);
    return id;
  }

  dismissToast(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  async start(key: string, note?: string) {
    try {
      const result = await api.startTimer(key, note);
      if (result.discarded.length) this.toast("Kurzer Eintrag wurde verworfen", "info");
      return result;
    } catch (e) {
      this.toast(String(e), "error");
      throw e;
    }
  }

  async stop(entryId?: number, at?: string, note?: string) {
    try {
      const results = await api.stopTimer(entryId, at, note);
      if (results.some((r) => r.discarded)) this.toast("Eintrag war kürzer als das Minimum und wurde verworfen", "info");
      return results;
    } catch (e) {
      this.toast(String(e), "error");
      throw e;
    }
  }

  async toggleFavorite(issue: Issue) {
    try {
      const result = await api.setFavorite(issue.key, !issue.favorite);
      if (!result.accepted) this.favoriteLimit = result;
    } catch (e) {
      this.toast(String(e), "error");
    }
  }

  async saveSettings(patch: (s: Settings) => void) {
    if (!this.settings) return;
    const next = structuredClone($state.snapshot(this.settings));
    patch(next);
    try {
      this.settings = await api.saveSettings(next);
    } catch (e) {
      this.toast(String(e), "error");
    }
  }
}

export const store = new AppStore();
