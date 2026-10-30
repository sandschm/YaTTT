<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type Issue } from "../lib/api";
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(""),
    placeholder = "Ticket suchen oder Key eingeben …",
    onpick,
    autofocus = false,
  }: { value?: string; placeholder?: string; onpick: (issue: Issue) => void; autofocus?: boolean } = $props();

  let query = $state("");
  let open = $state(false);
  let highlighted = $state(0);
  let loadingKey = $state(false);
  let input: HTMLInputElement | undefined = $state();

  const selected = $derived(value ? store.issue(value) : undefined);

  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const recentFirst = [...store.issues].sort((a, b) => Number(b.favorite) - Number(a.favorite) || a.key.localeCompare(b.key));
    if (!q) return recentFirst.slice(0, 12);
    return recentFirst
      .filter((i) => i.key.toLowerCase().includes(q) || i.summary.toLowerCase().includes(q))
      .slice(0, 12);
  });

  const looksLikeKey = $derived(/^[a-z][a-z0-9_]*-\d+$/i.test(query.trim()));

  function choose(issue: Issue) {
    value = issue.key;
    query = "";
    open = false;
    onpick(issue);
  }

  async function addByKey() {
    if (!looksLikeKey) return;
    loadingKey = true;
    try {
      const issue = await api.addIssueByKey(query.trim());
      choose(issue);
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      loadingKey = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      highlighted = Math.min(highlighted + 1, matches.length - 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      highlighted = Math.max(highlighted - 1, 0);
      e.preventDefault();
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (matches[highlighted]) choose(matches[highlighted]);
      else if (looksLikeKey) addByKey();
    } else if (e.key === "Escape") {
      open = false;
      input?.blur();
    }
  }

  $effect(() => {
    if (autofocus && input) input.focus();
  });
</script>

<div class="picker">
  {#if selected && !open}
    <button class="selected" type="button" onclick={() => { open = true; setTimeout(() => input?.focus()); }}>
      <span class="dot" style="background:{store.color(selected.projectKey)}"></span>
      <span class="key mono">{selected.key}</span>
      <span class="ellipsis grow">{selected.summary}</span>
      <Icon name="edit" size={14} />
    </button>
  {:else}
    <div class="input-wrap">
      <Icon name="search" size={15} />
      <input
        bind:this={input}
        type="text"
        {placeholder}
        bind:value={query}
        onfocus={() => (open = true)}
        onblur={() => setTimeout(() => (open = false), 150)}
        onkeydown={onKey}
        oninput={() => (highlighted = 0)}
      />
    </div>
    {#if open}
      <div class="dropdown card">
        {#each matches as issue, i (issue.key)}
          <button type="button" class="option" class:active={i === highlighted} onmousedown={() => choose(issue)}>
            <span class="dot" style="background:{store.color(issue.projectKey)}"></span>
            <span class="key mono">{issue.key}</span>
            <span class="ellipsis grow">{issue.summary}</span>
            {#if issue.favorite}<span class="fav"><Icon name="star" size={12} /></span>{/if}
          </button>
        {/each}
        {#if looksLikeKey && !matches.some((m) => m.key.toLowerCase() === query.trim().toLowerCase())}
          <button type="button" class="option add" onmousedown={addByKey} disabled={loadingKey}>
            <Icon name="plus" size={14} />
            <span>{query.trim().toUpperCase()} aus Jira laden</span>
          </button>
        {:else if !matches.length}
          <div class="empty faint">Kein Treffer. Einen Ticket-Key wie ABC-123 eingeben, um ihn aus Jira zu laden.</div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .picker {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 8px;
    color: var(--text-3);
  }
  .input-wrap:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .input-wrap input {
    border: none;
    background: transparent;
    flex: 1;
    padding: 7px 0;
    box-shadow: none !important;
    color: var(--text);
  }
  .selected {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 7px 9px;
    cursor: text;
    color: var(--text-3);
  }
  .selected .grow {
    color: var(--text);
  }
  .key {
    color: var(--text-2);
    font-size: 12.5px;
    flex-shrink: 0;
  }
  .dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 20;
    max-height: 320px;
    overflow: auto;
    padding: 4px;
    box-shadow: var(--shadow-lg);
  }
  .option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 7px 8px;
    border: none;
    background: transparent;
    border-radius: 6px;
    cursor: pointer;
  }
  .option:hover,
  .option.active {
    background: var(--surface-2);
  }
  .option.add {
    color: var(--accent);
  }
  .fav {
    color: var(--warning);
  }
  .empty {
    padding: 10px 8px;
    font-size: 12.5px;
  }
</style>
