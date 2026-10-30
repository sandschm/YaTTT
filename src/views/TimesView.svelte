<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type TimeEntry } from "../lib/api";
  import { addDays, formatDate, formatDuration, formatSigned, formatTime, startOfDay, startOfWeek, toDateKey } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import EntryEditor from "../components/EntryEditor.svelte";

  let mode = $state<"list" | "week">("list");
  let anchor = $state(startOfDay(new Date()));
  let entries = $state<TimeEntry[]>([]);
  let loading = $state(false);
  let editing = $state<TimeEntry | null | undefined>(undefined); // undefined = closed, null = new
  let editorDate = $state(new Date());
  let booking = $state(false);

  const firstDay = $derived(store.settings?.work.firstDayOfWeek ?? 0);
  const weekStart = $derived(startOfWeek(anchor, firstDay));
  const weekEnd = $derived(addDays(weekStart, 7));
  const days = $derived(Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)));
  const todayKey = toDateKey(new Date());

  async function load() {
    loading = true;
    try {
      entries = await api.listEntries(weekStart.toISOString(), weekEnd.toISOString());
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void store.entriesVersion;
    void weekStart;
    load();
  });

  function target(d: Date): number {
    const idx = (d.getDay() + 6) % 7;
    return (store.settings?.work.hoursPerWeekday[idx] ?? 0) * 3600;
  }

  function duration(e: TimeEntry): number {
    return store.elapsed(e);
  }

  const byDay = $derived.by(() => {
    void store.now;
    const map = new Map<string, { entries: TimeEntry[]; seconds: number }>();
    for (const d of days) map.set(toDateKey(d), { entries: [], seconds: 0 });
    for (const e of entries) {
      const key = toDateKey(new Date(e.startedAt));
      const bucket = map.get(key);
      if (!bucket) continue;
      bucket.entries.push(e);
      bucket.seconds += duration(e);
    }
    return map;
  });

  const weekTotal = $derived([...byDay.values()].reduce((s, d) => s + d.seconds, 0));
  const weekTarget = $derived(days.filter((d) => toDateKey(d) <= todayKey).reduce((s, d) => s + target(d), 0));
  const unbooked = $derived(entries.filter((e) => e.endedAt && e.syncState !== "synced" && e.syncState !== "pending"));

  function syncLabel(e: TimeEntry): { text: string; cls: string; title: string } {
    switch (e.syncState) {
      case "synced":
        return { text: "gebucht", cls: "success", title: `Jira-Worklog ${e.jiraWorklogId ?? ""}${e.tempoWorklogId ? ", Tempo " + e.tempoWorklogId : ""} – ${formatDuration(e.bookedSeconds ?? 0)} h` };
      case "pending":
        return { text: "wird gebucht", cls: "accent", title: "Buchung läuft" };
      case "modified":
        return { text: "geändert", cls: "warning", title: "Nach der Buchung geändert – wird aktualisiert" };
      case "failed":
        return { text: "Fehler", cls: "danger", title: e.syncError ?? "Buchung fehlgeschlagen" };
      default:
        return { text: "lokal", cls: "", title: "Noch nicht nach Jira gebucht" };
    }
  }

  async function book(ids: number[]) {
    booking = true;
    try {
      await api.bookEntries(ids);
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      booking = false;
    }
  }

  async function remove(e: TimeEntry) {
    await api.deleteEntry(e.id);
    store.toast(`Eintrag ${e.issueKey} gelöscht`, "info", { label: "Rückgängig", run: async () => { await api.restoreEntry(e.id); } });
  }

  async function duplicate(e: TimeEntry) {
    if (!e.endedAt) return;
    await api.addEntry({ issueKey: e.issueKey, startedAt: e.startedAt, endedAt: e.endedAt, note: e.note, billable: e.billable });
  }

  async function openIssue(key: string) {
    const url = await api.getIssueUrl(key);
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  function newEntry(d: Date = new Date()) {
    editorDate = d;
    editing = null;
  }

  // ---- week view geometry
  const hourHeight = 44;
  const hours = $derived.by(() => {
    let min = 8;
    let max = 18;
    for (const e of entries) {
      const s = new Date(e.startedAt);
      const en = e.endedAt ? new Date(e.endedAt) : new Date(store.now);
      min = Math.min(min, s.getHours());
      max = Math.max(max, en.getHours() + 1);
    }
    return { min: Math.max(0, min), max: Math.min(24, max) };
  });

  function blockStyle(e: TimeEntry, day: Date): string {
    const dayStart = startOfDay(day).getTime();
    const s = Math.max(new Date(e.startedAt).getTime(), dayStart);
    const en = Math.min(e.endedAt ? new Date(e.endedAt).getTime() : store.now, dayStart + 86400000);
    const top = ((s - dayStart) / 3600000 - hours.min) * hourHeight;
    const height = Math.max(18, ((en - s) / 3600000) * hourHeight);
    return `top:${top}px;height:${height}px;--c:${store.color(store.issue(e.issueKey)?.projectKey ?? e.issueKey.split("-")[0])}`;
  }

  function nowLine(): number {
    const n = new Date(store.now);
    return ((n.getHours() + n.getMinutes() / 60) - hours.min) * hourHeight;
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>Zeiten</h1>
    <div class="nav row">
      <button class="btn icon" type="button" onclick={() => (anchor = addDays(anchor, -7))} aria-label="Vorherige Woche"><Icon name="chevronLeft" /></button>
      <button class="btn sm" type="button" onclick={() => (anchor = startOfDay(new Date()))}>Heute</button>
      <button class="btn icon" type="button" onclick={() => (anchor = addDays(anchor, 7))} aria-label="Nächste Woche"><Icon name="chevronRight" /></button>
      <span class="range muted">{formatDate(weekStart, { day: "2-digit", month: "2-digit" })} – {formatDate(addDays(weekEnd, -1), { day: "2-digit", month: "2-digit", year: "numeric" })}</span>
    </div>
    <div class="grow"></div>
    <div class="totals mono">
      <span>{formatDuration(weekTotal, store.timeFormat)} h</span>
      {#if weekTarget > 0}
        <span class="badge" class:success={weekTotal >= weekTarget} class:warning={weekTotal < weekTarget}>{formatSigned(weekTotal - weekTarget, store.timeFormat)}</span>
      {/if}
    </div>
    <div class="seg">
      <button class:on={mode === "list"} type="button" onclick={() => (mode = "list")}>Liste</button>
      <button class:on={mode === "week"} type="button" onclick={() => (mode = "week")}>Woche</button>
    </div>
    <button class="btn primary sm" type="button" onclick={() => newEntry()}><Icon name="plus" size={14} /> Eintrag</button>
  </div>

  <div class="view-body">
    {#if mode === "list"}
      {#each days as day (toDateKey(day))}
        {@const key = toDateKey(day)}
        {@const bucket = byDay.get(key)!}
        {@const tgt = target(day)}
        {#if bucket.entries.length || key === todayKey || (tgt > 0 && key < todayKey)}
          <section class="day" class:today={key === todayKey}>
            <header class="day-head">
              <strong>{formatDate(day)}</strong>
              {#if key === todayKey}<span class="badge accent">Heute</span>{/if}
              <span class="grow"></span>
              {#if tgt > 0 && key <= todayKey}
                <span class="faint small">Soll {formatDuration(tgt, store.timeFormat)}</span>
                <span class="badge" class:success={bucket.seconds >= tgt} class:warning={bucket.seconds < tgt && bucket.seconds > 0} class:danger={bucket.seconds === 0 && key < todayKey}>{formatSigned(bucket.seconds - tgt, store.timeFormat)}</span>
              {/if}
              <span class="mono total">{formatDuration(bucket.seconds, store.timeFormat)}</span>
              <button class="btn ghost icon sm" type="button" onclick={() => newEntry(day)} title="Eintrag an diesem Tag anlegen" aria-label="Eintrag anlegen"><Icon name="plus" size={14} /></button>
            </header>
            {#if bucket.entries.length}
              <div class="card">
                {#each bucket.entries as e (e.id)}
                  {@const issue = store.issue(e.issueKey)}
                  {@const sync = syncLabel(e)}
                  <div class="entry" class:running={!e.endedAt}>
                    <span class="time mono">{formatTime(e.startedAt)} – {e.endedAt ? formatTime(e.endedAt) : "läuft"}</span>
                    <span class="dur mono">{formatDuration(duration(e), store.timeFormat)}</span>
                    <span class="dot" style="background:{store.color(issue?.projectKey ?? e.issueKey.split('-')[0])}"></span>
                    <div class="main grow">
                      <div class="row">
                        <button class="key mono" type="button" onclick={() => openIssue(e.issueKey)}>{e.issueKey}</button>
                        <span class="ellipsis">{issue?.summary ?? ""}</span>
                      </div>
                      {#if e.note}<div class="note ellipsis">{e.note}</div>{/if}
                    </div>
                    {#if !e.billable}<span class="badge" title="Nicht abrechenbar">n. abr.</span>{/if}
                    <span class="badge {sync.cls}" title={sync.title}>{sync.text}</span>
                    <div class="actions row">
                      {#if e.endedAt && e.syncState !== "synced" && e.syncState !== "pending"}
                        <button class="btn ghost icon sm" type="button" onclick={() => book([e.id])} title="Nach Jira buchen" aria-label="Buchen" disabled={booking}><Icon name="upload" size={14} /></button>
                      {/if}
                      <button class="btn ghost icon sm" type="button" onclick={() => (editing = e)} title="Bearbeiten" aria-label="Bearbeiten"><Icon name="edit" size={14} /></button>
                      {#if e.endedAt}
                        <button class="btn ghost icon sm" type="button" onclick={() => duplicate(e)} title="Duplizieren" aria-label="Duplizieren"><Icon name="copy" size={14} /></button>
                      {/if}
                      <button class="btn ghost icon sm danger" type="button" onclick={() => remove(e)} title="Löschen" aria-label="Löschen"><Icon name="trash" size={14} /></button>
                    </div>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="faint small nothing">Keine Zeiten erfasst.</div>
            {/if}
          </section>
        {/if}
      {/each}
    {:else}
      <div class="week card">
        <div class="week-head">
          <div class="gutter"></div>
          {#each days as day (toDateKey(day))}
            <div class="col-head" class:today={toDateKey(day) === todayKey}>
              <div>{formatDate(day, { weekday: "short" })}</div>
              <div class="faint">{formatDate(day, { day: "2-digit", month: "2-digit" })}</div>
              <div class="mono small">{formatDuration(byDay.get(toDateKey(day))?.seconds ?? 0, store.timeFormat)}</div>
            </div>
          {/each}
        </div>
        <div class="week-body" style="height:{(hours.max - hours.min) * hourHeight}px">
          <div class="gutter">
            {#each Array.from({ length: hours.max - hours.min }, (_, i) => hours.min + i) as h (h)}
              <div class="hour" style="top:{(h - hours.min) * hourHeight}px">{String(h).padStart(2, "0")}:00</div>
            {/each}
          </div>
          {#each days as day (toDateKey(day))}
            {@const key = toDateKey(day)}
            <div class="col" class:today={key === todayKey} role="presentation" ondblclick={() => newEntry(day)}>
              {#each Array.from({ length: hours.max - hours.min }, (_, i) => i) as i (i)}
                <div class="gridline" style="top:{i * hourHeight}px"></div>
              {/each}
              {#if key === todayKey}
                <div class="nowline" style="top:{nowLine()}px"></div>
              {/if}
              {#each byDay.get(key)?.entries ?? [] as e (e.id)}
                <button class="block" class:running={!e.endedAt} style={blockStyle(e, day)} type="button" onclick={() => (editing = e)} title="{e.issueKey} {formatTime(e.startedAt)}–{e.endedAt ? formatTime(e.endedAt) : 'läuft'}{e.note ? ' · ' + e.note : ''}">
                  <span class="mono">{e.issueKey}</span>
                  <span class="mono small">{formatDuration(duration(e), store.timeFormat)}</span>
                </button>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <footer class="foot no-print">
    <span class="muted small">
      {#if unbooked.length}{unbooked.length} ungebuchte {unbooked.length === 1 ? "Eintrag" : "Einträge"} in dieser Woche{:else}Alle abgeschlossenen Einträge dieser Woche sind gebucht.{/if}
      {#if loading} · lade …{/if}
    </span>
    <span class="grow"></span>
    <button class="btn sm" type="button" onclick={() => api.syncNow()} disabled={!store.connected} title="Ausstehende Buchungen und Löschungen übertragen"><Icon name="refresh" size={14} /> Synchronisieren</button>
    <button class="btn primary sm" type="button" onclick={() => book(unbooked.map((e) => e.id))} disabled={!unbooked.length || booking || !store.connected}><Icon name="upload" size={14} /> Woche buchen</button>
  </footer>
</div>

{#if editing !== undefined}
  <EntryEditor entry={editing} defaultDate={editorDate} onclose={() => (editing = undefined)} onsaved={() => (editing = undefined)} />
{/if}

<style>
  .small {
    font-size: 12px;
  }
  .nav {
    gap: 4px;
  }
  .range {
    margin-left: 8px;
    white-space: nowrap;
  }
  .totals {
    display: flex;
    gap: 8px;
    align-items: center;
    font-weight: 600;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    border: none;
    background: var(--surface);
    padding: 5px 12px;
    cursor: pointer;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .day {
    margin-bottom: 16px;
  }
  .day-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
  }
  .day-head .total {
    font-weight: 600;
    min-width: 50px;
    text-align: right;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .entry:last-child {
    border-bottom: none;
  }
  .entry:hover {
    background: var(--surface-2);
  }
  .entry.running {
    background: color-mix(in srgb, var(--running-soft) 50%, var(--surface));
  }
  .time {
    width: 110px;
    color: var(--text-2);
    font-size: 12.5px;
    flex-shrink: 0;
  }
  .dur {
    width: 52px;
    text-align: right;
    font-weight: 600;
    flex-shrink: 0;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .key {
    background: none;
    border: none;
    padding: 0;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12.5px;
    flex-shrink: 0;
  }
  .key:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .note {
    font-size: 12px;
    color: var(--text-3);
  }
  .actions {
    gap: 2px;
    opacity: 0.35;
  }
  .entry:hover .actions {
    opacity: 1;
  }
  .nothing {
    padding: 4px 8px 0;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  /* week view */
  .week {
    overflow: hidden;
  }
  .week-head {
    display: grid;
    grid-template-columns: 56px repeat(7, 1fr);
    border-bottom: 1px solid var(--border);
  }
  .col-head {
    padding: 8px 6px;
    text-align: center;
    font-size: 12.5px;
    border-left: 1px solid var(--border);
  }
  .col-head.today {
    background: var(--accent-soft);
  }
  .week-body {
    display: grid;
    grid-template-columns: 56px repeat(7, 1fr);
    position: relative;
  }
  .gutter {
    position: relative;
  }
  .hour {
    position: absolute;
    right: 6px;
    transform: translateY(-50%);
    font-size: 11px;
    color: var(--text-3);
  }
  .col {
    position: relative;
    border-left: 1px solid var(--border);
  }
  .col.today {
    background: color-mix(in srgb, var(--accent-soft) 35%, transparent);
  }
  .gridline {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px dashed var(--border);
  }
  .nowline {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 2px solid var(--running);
    z-index: 2;
  }
  .block {
    position: absolute;
    left: 3px;
    right: 3px;
    background: color-mix(in srgb, var(--c) 18%, var(--surface));
    border: 1px solid var(--c);
    border-left-width: 4px;
    border-radius: 5px;
    padding: 2px 5px;
    text-align: left;
    overflow: hidden;
    font-size: 11.5px;
    display: flex;
    flex-direction: column;
    cursor: pointer;
    z-index: 1;
  }
  .block.running {
    border-style: dashed;
  }
</style>
