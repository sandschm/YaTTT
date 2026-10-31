<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type Issue, type TimeEntry } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { formatDuration, formatTime, fromLocalInput, toLocalInput } from "../lib/format";
  import Icon from "./Icon.svelte";
  import IssuePicker from "./IssuePicker.svelte";

  let pickerValue = $state("");
  let editingStart = $state<number | null>(null);
  let startDraft = $state("");
  let noteDrafts = $state<Record<number, string>>({});
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  let todaySeconds = $state(0);
  let loadedAt = $state(Date.now());

  const running = $derived(store.running);

  async function loadToday() {
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const end = new Date(start);
    end.setDate(end.getDate() + 1);
    const entries = await api.listEntries(start.toISOString(), end.toISOString());
    const nowMs = Date.now();
    todaySeconds = entries.reduce((sum, e) => {
      const s = Math.max(new Date(e.startedAt).getTime(), start.getTime());
      const en = e.endedAt ? new Date(e.endedAt).getTime() : nowMs;
      return sum + Math.max(0, (Math.min(en, end.getTime()) - s) / 1000);
    }, 0);
    loadedAt = nowMs;
  }

  $effect(() => {
    void store.entriesVersion;
    void store.running;
    loadToday();
  });

  // Live total: add the seconds elapsed since the last load for every running timer.
  const liveToday = $derived.by(() => {
    const delta = Math.max(0, Math.floor((store.now - loadedAt) / 1000));
    return todaySeconds + delta * running.length;
  });
  $effect(() => {
    void store.now;
    if (running.length) {
      const id = setInterval(loadToday, 60000);
      return () => clearInterval(id);
    }
  });

  async function onPick(issue: Issue) {
    await store.start(issue.key);
    pickerValue = "";
  }

  let noteAsk = $state<TimeEntry | null>(null);
  let noteAskText = $state("");

  async function stop(entry: TimeEntry) {
    const note = noteDrafts[entry.id] ?? entry.note;
    if (store.settings?.timer.askNoteOnStop && !note.trim()) {
      noteAsk = entry;
      noteAskText = "";
      return;
    }
    await store.stop(entry.id, undefined, note);
  }

  async function confirmNoteAndStop() {
    if (!noteAsk) return;
    const entry = noteAsk;
    noteAsk = null;
    await store.stop(entry.id, undefined, noteAskText.trim());
  }

  function beginEditStart(entry: TimeEntry) {
    editingStart = entry.id;
    startDraft = toLocalInput(entry.startedAt);
  }

  async function commitStart(entry: TimeEntry) {
    if (editingStart !== entry.id) return;
    editingStart = null;
    if (!startDraft) return;
    try {
      await api.updateRunning(entry.id, fromLocalInput(startDraft));
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  function onNoteInput(entry: TimeEntry, value: string) {
    noteDrafts[entry.id] = value;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => api.updateRunning(entry.id, undefined, value).catch((e) => store.toast(String(e), "error")), 600);
  }

  function openIssue(key: string) {
    import("@tauri-apps/plugin-opener").then(async ({ openUrl }) => {
      const url = await api.getIssueUrl(key);
      openUrl(url).catch(() => window.open(url, "_blank"));
    }).catch(async () => window.open(await api.getIssueUrl(key), "_blank"));
  }
</script>

<div class="timerbar" class:active={running.length > 0}>
  {#if running.length === 0}
    <div class="idle">
      <div class="label row">
        <Icon name="clock" size={18} />
        <span>{t("timer.none")}</span>
      </div>
      <IssuePicker bind:value={pickerValue} onpick={onPick} placeholder={t("timer.pickPlaceholder")} />
    </div>
  {:else}
    <div class="runs">
      {#each running as entry (entry.id)}
        {@const issue = store.issue(entry.issueKey)}
        <div class="run">
          <span class="dot" style="background:{store.color(issue?.projectKey ?? entry.issueKey.split('-')[0])}"></span>
          <div class="info grow">
            <div class="row">
              <button class="key mono" type="button" onclick={() => openIssue(entry.issueKey)} title={t("timer.openInJira")}>{entry.issueKey}</button>
              <span class="ellipsis summary">{issue?.summary ?? ""}</span>
            </div>
            <div class="row meta">
              {#if editingStart === entry.id}
                <input
                  type="datetime-local"
                  bind:value={startDraft}
                  onblur={() => commitStart(entry)}
                  onkeydown={(e) => { if (e.key === "Enter") commitStart(entry); if (e.key === "Escape") editingStart = null; }}
                />
              {:else}
                <button class="link" type="button" onclick={() => beginEditStart(entry)} title={t("timer.editStart")}>{t("timer.since", { time: formatTime(entry.startedAt) })}</button>
              {/if}
              <input
                class="note"
                type="text"
                placeholder={t("timer.notePlaceholder")}
                value={noteDrafts[entry.id] ?? entry.note}
                oninput={(e) => onNoteInput(entry, (e.target as HTMLInputElement).value)}
              />
            </div>
          </div>
          <div class="elapsed mono">{formatDuration(store.elapsed(entry), "hoursMinutes", true)}</div>
          <button class="btn running icon" type="button" onclick={() => stop(entry)} title={t("timer.stop")} aria-label={t("timer.stop")}>
            <Icon name="stop" size={16} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
  {#if noteAsk}
    <div class="overlay" role="presentation">
      <div class="dialog" role="dialog" aria-modal="true">
        <h2>{t("timer.noteTitle")}</h2>
        <p>{t("timer.noteText", { key: noteAsk.issueKey })}</p>
        <textarea rows="3" bind:value={noteAskText} placeholder={t("timer.noteInput")} onkeydown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) confirmNoteAndStop(); if (e.key === "Escape") noteAsk = null; }}></textarea>
        <div class="actions">
          <button class="btn" type="button" onclick={() => (noteAsk = null)}>{t("common.cancel")}</button>
          <button class="btn" type="button" onclick={() => { noteAskText = ""; confirmNoteAndStop(); }}>{t("timer.stopWithoutNote")}</button>
          <button class="btn primary" type="button" onclick={confirmNoteAndStop}>{t("timer.stopBtn")}</button>
        </div>
      </div>
    </div>
  {/if}
  <div class="today">
    <span class="faint">{t("common.today")}</span>
    <span class="mono total">{formatDuration(liveToday, store.timeFormat)}</span>
  </div>
</div>

<style>
  .timerbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    min-height: 64px;
  }
  .timerbar.active {
    background: linear-gradient(90deg, var(--running-soft), var(--surface) 55%);
  }
  .idle {
    display: flex;
    align-items: center;
    gap: 14px;
    flex: 1;
    min-width: 0;
  }
  .label {
    color: var(--text-2);
    white-space: nowrap;
  }
  .runs {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .run {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .key {
    background: none;
    border: none;
    padding: 0;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12.5px;
  }
  .key:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .summary {
    font-weight: 600;
  }
  .meta {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .meta input[type="datetime-local"] {
    padding: 2px 6px;
    font-size: 12px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--text-2);
    cursor: pointer;
    white-space: nowrap;
  }
  .link:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .note {
    flex: 1;
    min-width: 120px;
    max-width: 460px;
    padding: 3px 8px;
    font-size: 12.5px;
    background: transparent;
    border-color: transparent;
  }
  .note:hover,
  .note:focus {
    background: var(--surface);
    border-color: var(--border);
  }
  .elapsed {
    font-size: 24px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--running);
    min-width: 110px;
    text-align: right;
  }
  .dialog textarea {
    width: 100%;
    resize: vertical;
  }
  .today {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    line-height: 1.2;
    padding-left: 16px;
    border-left: 1px solid var(--border);
    font-size: 12px;
  }
  .total {
    font-size: 16px;
    font-weight: 600;
  }
</style>
