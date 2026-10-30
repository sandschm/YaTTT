<script lang="ts">
  // Idle / sleep prompt, restore prompt after restart, exit confirmation and toasts.
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { formatDuration, formatTime } from "../lib/format";
  import Icon from "./Icon.svelte";

  const idle = $derived(store.idlePrompt);
  const restore = $derived(store.restorePrompt);
  const exit = $derived(store.exitRequested);

  const idleMinutes = $derived(idle ? Math.round((store.now - new Date(idle.idleSince).getTime()) / 60000) : 0);

  async function respondIdle(decision: "keep" | "stopAtIdleStart" | "discardIdle") {
    try {
      await api.respondIdle(decision);
    } catch (e) {
      store.toast(String(e), "error");
    }
    store.idlePrompt = null;
  }

  async function respondRestore(entryId: number, decision: "keepRunning" | "stopAtLastSeen" | "stopNow" | "discard") {
    try {
      await api.respondRestore(entryId, decision);
    } catch (e) {
      store.toast(String(e), "error");
    }
    store.restorePrompt = await api.getRestorePrompt();
  }

  async function confirmExit(stop: boolean) {
    await api.confirmExit(stop);
    store.exitRequested = null;
  }

  async function cancelExit() {
    await api.cancelExit();
    store.exitRequested = null;
  }
</script>

{#if idle}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>{idle.reason === "sleep" ? "Der Rechner war im Ruhezustand" : "Du warst eine Weile inaktiv"}</h2>
      <p>
        Seit {formatTime(idle.idleSince)} ({formatDuration(idleMinutes * 60)} h) gab es keine Eingaben. Der Timer
        {#each idle.entryIds as id, i}{i > 0 ? ", " : " "}<strong>{store.running.find((e) => e.id === id)?.issueKey ?? id}</strong>{/each}
        lief währenddessen weiter. Was soll mit dieser Zeit passieren?
      </p>
      <div class="options">
        <button class="btn" type="button" onclick={() => respondIdle("keep")}>
          <Icon name="check" size={15} /> Zeit behalten
          <span class="hint">Ich habe weitergearbeitet.</span>
        </button>
        <button class="btn" type="button" onclick={() => respondIdle("discardIdle")}>
          <Icon name="bolt" size={15} /> Inaktive Zeit abziehen
          <span class="hint">Eintrag endet {formatTime(idle.idleSince)}, Timer läuft ab jetzt weiter.</span>
        </button>
        <button class="btn" type="button" onclick={() => respondIdle("stopAtIdleStart")}>
          <Icon name="stop" size={15} /> Timer um {formatTime(idle.idleSince)} stoppen
          <span class="hint">Nichts läuft weiter.</span>
        </button>
      </div>
    </div>
  </div>
{/if}

{#if restore && restore.entries.length}
  {@const entry = restore.entries[0]}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>Timer lief beim letzten Beenden noch</h2>
      <p>
        {#if restore.keptRunningOnExit}
          Beim Beenden wurde „Weiterlaufen lassen“ gewählt.
        {:else}
          YaTTT wurde beendet (oder der Rechner heruntergefahren), während der Timer für <strong>{entry.issueKey}</strong> lief.
        {/if}
        Gestartet {formatTime(entry.startedAt)}{restore.lastSeen ? `, zuletzt aktiv ${formatTime(restore.lastSeen)}` : ""}.
        Bis jetzt wären das {formatDuration(store.elapsed(entry))} h.
      </p>
      <div class="options">
        <button class="btn" type="button" onclick={() => respondRestore(entry.id, "keepRunning")}>
          <Icon name="play" size={15} /> Weiterlaufen lassen
          <span class="hint">Die Zeit zählt durchgehend bis jetzt.</span>
        </button>
        {#if restore.lastSeen}
          <button class="btn" type="button" onclick={() => respondRestore(entry.id, "stopAtLastSeen")}>
            <Icon name="stop" size={15} /> Bei letzter Aktivität stoppen ({formatTime(restore.lastSeen)})
          </button>
        {/if}
        <button class="btn" type="button" onclick={() => respondRestore(entry.id, "stopNow")}>
          <Icon name="stop" size={15} /> Jetzt stoppen
        </button>
        <button class="btn danger" type="button" onclick={() => respondRestore(entry.id, "discard")}>
          <Icon name="trash" size={15} /> Eintrag verwerfen
        </button>
      </div>
    </div>
  </div>
{/if}

{#if exit}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>Ein Timer läuft noch</h2>
      <p>
        {#if exit === "shutdown"}
          Das System möchte herunterfahren oder dich abmelden. YaTTT hat den Vorgang kurz angehalten, damit du entscheiden kannst.
          Nach deiner Wahl musst du das Herunterfahren erneut auslösen.
        {:else}
          Soll der Timer gestoppt werden oder weiterlaufen? Bei „Weiterlaufen lassen“ zählt die Zeit weiter und wird beim nächsten Start angezeigt.
        {/if}
      </p>
      <div class="actions">
        <button class="btn" type="button" onclick={cancelExit}>Abbrechen</button>
        <button class="btn" type="button" onclick={() => confirmExit(false)}>Weiterlaufen lassen und beenden</button>
        <button class="btn primary" type="button" onclick={() => confirmExit(true)}>Timer stoppen und beenden</button>
      </div>
    </div>
  </div>
{/if}

<div class="toasts no-print">
  {#each store.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">
      <span class="grow">{toast.message}</span>
      {#if toast.action}
        <button class="btn sm" type="button" onclick={() => { toast.action?.run(); store.dismissToast(toast.id); }}>{toast.action.label}</button>
      {/if}
      <button class="btn ghost sm icon" type="button" onclick={() => store.dismissToast(toast.id)} aria-label="Schließen"><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .options .btn {
    justify-content: flex-start;
    padding: 10px 12px;
    flex-wrap: wrap;
    text-align: left;
  }
  .options .hint {
    display: block;
    width: 100%;
    color: var(--text-3);
    font-size: 12px;
    margin-left: 21px;
    white-space: normal;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 60;
    max-width: 420px;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    border-left: 4px solid var(--accent);
  }
  .toast.success {
    border-left-color: var(--success);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
