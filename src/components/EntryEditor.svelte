<script lang="ts">
  import { untrack } from "svelte";
  import { store } from "../lib/store.svelte";
  import { api, type TimeEntry, type TimeEntryInput } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { formatDuration, fromLocalInput, parseDurationInput, toLocalInput } from "../lib/format";
  import IssuePicker from "./IssuePicker.svelte";

  let {
    entry = null,
    defaultDate = new Date(),
    onclose,
    onsaved,
  }: { entry?: TimeEntry | null; defaultDate?: Date; onclose: () => void; onsaved: (e: TimeEntry) => void } = $props();

  // The editor works on a copy of the initial values; props are read once on purpose.
  const initial = untrack(() => {
    const d = defaultDate;
    return {
      issueKey: entry?.issueKey ?? "",
      start: entry ? toLocalInput(entry.startedAt) : toLocalInput(new Date(d.getFullYear(), d.getMonth(), d.getDate(), 9, 0)),
      end: entry?.endedAt ? toLocalInput(entry.endedAt) : entry ? "" : toLocalInput(new Date(d.getFullYear(), d.getMonth(), d.getDate(), 10, 0)),
      note: entry?.note ?? "",
      billable: entry?.billable ?? store.settings?.timer.defaultBillable ?? true,
    };
  });

  let issueKey = $state(initial.issueKey);
  let start = $state(initial.start);
  let end = $state(initial.end);
  let note = $state(initial.note);
  let billable = $state(initial.billable);
  let durationText = $state("");
  let saving = $state(false);
  let error = $state("");

  const isRunning = $derived(!!entry && !entry.endedAt);
  const durationSeconds = $derived.by(() => {
    if (!start || !end) return 0;
    return Math.max(0, (new Date(end).getTime() - new Date(start).getTime()) / 1000);
  });

  function applyDuration() {
    const secs = parseDurationInput(durationText);
    if (secs == null || !start) return;
    end = toLocalInput(new Date(new Date(start).getTime() + secs * 1000));
    durationText = "";
  }

  async function save() {
    error = "";
    if (!issueKey) {
      error = t("editor.errTicket");
      return;
    }
    if (!start) {
      error = t("editor.errStart");
      return;
    }
    if (!isRunning && !end) {
      error = t("editor.errEnd");
      return;
    }
    if (end && new Date(end) < new Date(start)) {
      error = t("editor.errOrder");
      return;
    }
    const input: TimeEntryInput = {
      issueKey,
      startedAt: fromLocalInput(start),
      endedAt: end ? fromLocalInput(end) : null,
      note,
      billable,
    };
    saving = true;
    try {
      const saved = entry ? await api.updateEntry(entry.id, input) : await api.addEntry(input);
      onsaved(saved);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) save();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="overlay" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onclose(); }}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="entry-editor-title">
    <h2 id="entry-editor-title">{entry ? t("editor.edit") : t("editor.new")}</h2>
    {#if entry?.syncState === "synced"}
      <p class="badge warning">{t("editor.synced")}</p>
    {/if}
    <div class="field">
      <label for="entry-issue">{t("editor.ticket")}</label>
      <IssuePicker bind:value={issueKey} onpick={(i) => (issueKey = i.key)} autofocus={!entry} />
    </div>
    <div class="grid">
      <div class="field">
        <label for="entry-start">{t("editor.start")}</label>
        <input id="entry-start" type="datetime-local" bind:value={start} />
      </div>
      <div class="field">
        <label for="entry-end">{t("editor.end")}</label>
        <input id="entry-end" type="datetime-local" bind:value={end} disabled={isRunning} placeholder={isRunning ? t("editor.running") : ""} />
      </div>
      <div class="field">
        <label for="entry-duration">{t("editor.duration")}</label>
        <div class="row">
          <input id="entry-duration" type="text" placeholder={isRunning ? t("editor.running") : t("editor.durationPlaceholder")} bind:value={durationText} onblur={applyDuration} onkeydown={(e) => e.key === "Enter" && applyDuration()} disabled={isRunning} />
          <span class="mono muted">{isRunning ? "" : formatDuration(durationSeconds, "hoursMinutes")}</span>
        </div>
      </div>
    </div>
    <div class="field">
      <label for="entry-note">{t("editor.note")}</label>
      <textarea id="entry-note" rows="2" bind:value={note}></textarea>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={billable} />
      <span>{t("editor.billable")}</span>
    </label>
    {#if error}<div class="error">{error}</div>{/if}
    <div class="actions">
      <button class="btn" type="button" onclick={onclose}>{t("common.cancel")}</button>
      <button class="btn primary" type="button" onclick={save} disabled={saving}>{entry ? t("common.save") : t("editor.create")}</button>
    </div>
  </div>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 10px;
  }
  .error {
    color: var(--danger);
    font-size: 12.5px;
    margin-top: 6px;
  }
  @media (max-width: 560px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
