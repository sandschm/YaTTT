<script lang="ts">
  // Idle / sleep prompt, restore prompt after restart, exit confirmation and toasts.
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { formatDuration, formatTime } from "../lib/format";
  import Icon from "./Icon.svelte";

  const idle = $derived(store.idlePrompt);
  const restore = $derived(store.restorePrompt);
  const exit = $derived(store.exitRequested);
  const favoriteLimit = $derived(store.favoriteLimit);

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
      <h2>{idle.reason === "sleep" ? t("dialog.idle.sleepTitle") : t("dialog.idle.idleTitle")}</h2>
      <p>
        {t("dialog.idle.text", { time: formatTime(idle.idleSince), duration: formatDuration(idleMinutes * 60) })}
        {#each idle.entryIds as id, i}{i > 0 ? ", " : " "}<strong>{store.running.find((e) => e.id === id)?.issueKey ?? id}</strong>{/each}
        {t("dialog.idle.textEnd")}
      </p>
      <div class="options">
        <button class="btn" type="button" onclick={() => respondIdle("keep")}>
          <Icon name="check" size={15} /> {t("dialog.idle.keep")}
          <span class="hint">{t("dialog.idle.keepHint")}</span>
        </button>
        <button class="btn" type="button" onclick={() => respondIdle("discardIdle")}>
          <Icon name="bolt" size={15} /> {t("dialog.idle.discard")}
          <span class="hint">{t("dialog.idle.discardHint", { time: formatTime(idle.idleSince) })}</span>
        </button>
        <button class="btn" type="button" onclick={() => respondIdle("stopAtIdleStart")}>
          <Icon name="stop" size={15} /> {t("dialog.idle.stopAt", { time: formatTime(idle.idleSince) })}
          <span class="hint">{t("dialog.idle.stopHint")}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

{#if restore && restore.entries.length}
  {@const entry = restore.entries[0]}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>{t("dialog.restore.title")}</h2>
      <p>
        {#if restore.keptRunningOnExit}
          {t("dialog.restore.kept")}
        {:else}
          {t("dialog.restore.crashedBefore")} <strong>{entry.issueKey}</strong> {t("dialog.restore.crashedAfter")}
        {/if}
        {t("dialog.restore.started", { time: formatTime(entry.startedAt) })}{restore.lastSeen ? t("dialog.restore.lastActive", { time: formatTime(restore.lastSeen) }) : ""}.
        {t("dialog.restore.soFar", { duration: formatDuration(store.elapsed(entry)) })}
      </p>
      <div class="options">
        <button class="btn" type="button" onclick={() => respondRestore(entry.id, "keepRunning")}>
          <Icon name="play" size={15} /> {t("dialog.restore.keepRunning")}
          <span class="hint">{t("dialog.restore.keepRunningHint")}</span>
        </button>
        {#if restore.lastSeen}
          <button class="btn" type="button" onclick={() => respondRestore(entry.id, "stopAtLastSeen")}>
            <Icon name="stop" size={15} /> {t("dialog.restore.stopAtLastSeen", { time: formatTime(restore.lastSeen) })}
          </button>
        {/if}
        <button class="btn" type="button" onclick={() => respondRestore(entry.id, "stopNow")}>
          <Icon name="stop" size={15} /> {t("dialog.restore.stopNow")}
        </button>
        <button class="btn danger" type="button" onclick={() => respondRestore(entry.id, "discard")}>
          <Icon name="trash" size={15} /> {t("dialog.restore.discard")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if favoriteLimit}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>{t("dialog.fav.title", { limit: favoriteLimit.limit })}</h2>
      <p>{t("dialog.fav.text", { count: favoriteLimit.count, limit: favoriteLimit.limit })}</p>
      <div class="actions">
        <button class="btn primary" type="button" onclick={() => (store.favoriteLimit = null)}>{t("dialog.fav.ok")}</button>
      </div>
    </div>
  </div>
{/if}

{#if exit}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>{t("dialog.exit.title")}</h2>
      <p>{exit === "shutdown" ? t("dialog.exit.shutdown") : t("dialog.exit.quit")}</p>
      <div class="actions">
        <button class="btn" type="button" onclick={cancelExit}>{t("common.cancel")}</button>
        <button class="btn" type="button" onclick={() => confirmExit(false)}>{t("dialog.exit.keepAndQuit")}</button>
        <button class="btn primary" type="button" onclick={() => confirmExit(true)}>{t("dialog.exit.stopAndQuit")}</button>
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
      <button class="btn ghost sm icon" type="button" onclick={() => store.dismissToast(toast.id)} aria-label={t("common.close")}><Icon name="x" size={14} /></button>
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
