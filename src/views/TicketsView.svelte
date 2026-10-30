<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type Issue } from "../lib/api";
  import { formatDuration, relativeTime } from "../lib/format";
  import Icon from "../components/Icon.svelte";

  let search = $state("");
  let sprintFilter = $state<string>("all");
  let typeFilter = $state("all");
  let statusFilter = $state<"open" | "done" | "all">("open");
  let onlyMine = $state(false);
  let favoritesOnly = $state(false);
  let addKey = $state("");
  let adding = $state(false);

  const refresh = $derived(store.snapshot?.refresh);
  const selection = $derived(store.settings?.selection);
  const hasSelection = $derived(!!selection && (selection.projectKeys.length > 0 || selection.sprintIds.length > 0));

  const types = $derived([...new Set(store.issues.map((i) => i.issueType))].sort());
  const sprints = $derived.by(() => {
    const map = new Map<number, string>();
    for (const i of store.issues) if (i.sprintId != null) map.set(i.sprintId, i.sprintName ?? `Sprint ${i.sprintId}`);
    return [...map.entries()].sort((a, b) => a[1].localeCompare(b[1]));
  });

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    const me = store.auth.user?.id;
    return store.issues.filter((i) => {
      if (q && !i.key.toLowerCase().includes(q) && !i.summary.toLowerCase().includes(q)) return false;
      if (sprintFilter === "none" && i.sprintId != null) return false;
      if (sprintFilter !== "all" && sprintFilter !== "none" && String(i.sprintId) !== sprintFilter) return false;
      if (typeFilter !== "all" && i.issueType !== typeFilter) return false;
      if (statusFilter === "open" && i.statusCategory === "done" && !store.runningFor(i.key)) return false;
      if (statusFilter === "done" && i.statusCategory !== "done") return false;
      if (onlyMine && i.assigneeId !== me) return false;
      if (favoritesOnly && !i.favorite) return false;
      return true;
    });
  });

  interface Row {
    issue: Issue;
    depth: number;
  }
  interface Group {
    key: string;
    name: string;
    color: string;
    rows: Row[];
    seconds: number;
  }

  const groups = $derived.by(() => {
    const byProject = new Map<string, Issue[]>();
    for (const i of filtered) {
      if (!byProject.has(i.projectKey)) byProject.set(i.projectKey, []);
      byProject.get(i.projectKey)!.push(i);
    }
    const selectedKeys = selection?.projectKeys ?? [];
    const result: Group[] = [];
    for (const [key, issues] of byProject) {
      const present = new Set(issues.map((i) => i.key));
      const children = new Map<string, Issue[]>();
      const roots: Issue[] = [];
      for (const i of issues) {
        if (i.parentKey && present.has(i.parentKey)) {
          if (!children.has(i.parentKey)) children.set(i.parentKey, []);
          children.get(i.parentKey)!.push(i);
        } else roots.push(i);
      }
      const sortFn = (a: Issue, b: Issue) => Number(b.favorite) - Number(a.favorite) || Number(!!store.runningFor(b.key)) - Number(!!store.runningFor(a.key)) || a.key.localeCompare(b.key, undefined, { numeric: true });
      roots.sort(sortFn);
      const rows: Row[] = [];
      for (const r of roots) {
        rows.push({ issue: r, depth: 0 });
        for (const c of (children.get(r.key) ?? []).sort(sortFn)) rows.push({ issue: c, depth: 1 });
      }
      const project = store.projectMap.get(key);
      const seconds = issues.reduce((s, i) => s + tracked(i.key), 0);
      result.push({ key, name: project?.name ?? key, color: store.color(key), rows, seconds });
    }
    result.sort((a, b) => {
      const ai = selectedKeys.indexOf(a.key);
      const bi = selectedKeys.indexOf(b.key);
      if (ai !== -1 || bi !== -1) return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
      return a.name.localeCompare(b.name);
    });
    return result;
  });

  function tracked(key: string): number {
    const base = store.snapshot?.trackedSeconds[key] ?? 0;
    const running = store.runningFor(key);
    if (!running) return base;
    // trackedSeconds already includes the running entry up to snapshot time; add the delta since then.
    void store.now;
    return base + Math.max(0, store.elapsed(running) - (running ? elapsedAtSnapshot(running.id) : 0));
  }

  let snapshotTime = $state(Date.now());
  $effect(() => {
    void store.snapshot;
    snapshotTime = Date.now();
  });
  function elapsedAtSnapshot(entryId: number): number {
    const e = store.running.find((r) => r.id === entryId);
    if (!e) return 0;
    return Math.max(0, Math.floor((snapshotTime - new Date(e.startedAt).getTime()) / 1000));
  }

  async function toggle(issue: Issue) {
    const running = store.runningFor(issue.key);
    if (running) await store.stop(running.id);
    else await store.start(issue.key);
  }

  async function toggleFavorite(issue: Issue) {
    await api.setFavorite(issue.key, !issue.favorite);
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

  async function doRefresh() {
    try {
      await api.refreshIssues();
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  async function addIssue() {
    if (!addKey.trim()) return;
    adding = true;
    try {
      const issue = await api.addIssueByKey(addKey.trim());
      addKey = "";
      store.toast(`${issue.key} hinzugefügt`, "success");
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      adding = false;
    }
  }

  function typeClass(type: string): string {
    const t = type.toLowerCase();
    if (t.includes("bug")) return "bug";
    if (t.includes("story")) return "story";
    if (t.includes("sub")) return "subtask";
    if (t.includes("epic")) return "epic";
    return "task";
  }

  function estimatePct(issue: Issue): number | null {
    if (!issue.originalEstimateS) return null;
    return Math.min(150, (tracked(issue.key) / issue.originalEstimateS) * 100);
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>Tickets</h1>
    <span class="badge">{filtered.length}</span>
    <div class="grow"></div>
    {#if refresh}
      <span class="faint small">
        {#if refresh.inProgress}Aktualisiere …{:else if refresh.lastError}<span class="badge danger" title={refresh.lastError}>Fehler</span>{:else}Aktualisiert {relativeTime(refresh.lastSuccess)}{/if}
      </span>
    {/if}
    <button class="btn icon" type="button" onclick={doRefresh} disabled={refresh?.inProgress || !store.connected} title="Jetzt aktualisieren" aria-label="Jetzt aktualisieren">
      <span class:spin={refresh?.inProgress}><Icon name="refresh" size={16} /></span>
    </button>
  </div>

  <div class="toolbar no-print">
    <div class="search row grow">
      <Icon name="search" size={15} />
      <input type="search" placeholder="Suchen (Key oder Titel) …" bind:value={search} />
    </div>
    <select bind:value={sprintFilter} title="Sprint">
      <option value="all">Alle Sprints</option>
      {#each sprints as [id, name] (id)}
        <option value={String(id)}>{name}</option>
      {/each}
      <option value="none">Ohne Sprint</option>
    </select>
    <select bind:value={typeFilter} title="Tickettyp">
      <option value="all">Alle Typen</option>
      {#each types as t (t)}
        <option value={t}>{t}</option>
      {/each}
    </select>
    <select bind:value={statusFilter} title="Status">
      <option value="open">Offen</option>
      <option value="done">Erledigt</option>
      <option value="all">Alle</option>
    </select>
    <button class="btn sm" class:on={onlyMine} type="button" onclick={() => (onlyMine = !onlyMine)}><Icon name="user" size={14} /> Nur meine</button>
    <button class="btn sm" class:on={favoritesOnly} type="button" onclick={() => (favoritesOnly = !favoritesOnly)}><Icon name="star" size={14} /> Favoriten</button>
  </div>

  <div class="view-body">
    {#if !store.connected}
      <div class="empty card">
        <Icon name="link" size={28} />
        <h3>Nicht mit Jira verbunden</h3>
        <p class="muted">Hinterlege die Jira-URL und melde dich an, damit Projekte und Tickets geladen werden.</p>
        <button class="btn primary" type="button" onclick={() => (store.view = "settings")}>Zu den Einstellungen</button>
      </div>
    {:else if !hasSelection && store.issues.length === 0}
      <div class="empty card">
        <Icon name="list" size={28} />
        <h3>Keine Projekte oder Sprints gewählt</h3>
        <p class="muted">Wähle in den Einstellungen Projekte aus. Die Tickets der offenen Sprints werden dann automatisch geladen.</p>
        <button class="btn primary" type="button" onclick={() => (store.view = "settings")}>Projekte wählen</button>
      </div>
    {:else}
      {#each groups as group (group.key)}
        <section class="group">
          <header class="group-head">
            <span class="dot" style="background:{group.color}"></span>
            <strong>{group.name}</strong>
            <span class="faint mono">{group.key}</span>
            <span class="grow"></span>
            <span class="faint small">{group.rows.length} Tickets · {formatDuration(group.seconds, store.timeFormat)} h erfasst</span>
          </header>
          <div class="rows card">
            {#each group.rows as { issue, depth } (issue.key)}
              {@const running = store.runningFor(issue.key)}
              {@const pct = estimatePct(issue)}
              <div class="issue" class:running={!!running} class:child={depth > 0} style="--c:{group.color}">
                <button class="star" class:on={issue.favorite} type="button" onclick={() => toggleFavorite(issue)} title={issue.favorite ? "Favorit entfernen" : "Als Favorit markieren"} aria-label="Favorit">
                  <Icon name="star" size={15} />
                </button>
                <span class="type {typeClass(issue.issueType)}" title={issue.issueType}>{issue.issueType.slice(0, 1)}</span>
                <div class="main grow">
                  <div class="row">
                    <button class="key mono" type="button" onclick={() => openIssue(issue.key)} title="In Jira öffnen">{issue.key}</button>
                    <span class="summary ellipsis">{issue.summary}</span>
                  </div>
                  <div class="row meta">
                    <span class="badge" class:success={issue.statusCategory === "done"} class:accent={issue.statusCategory === "indeterminate"}>{issue.status}</span>
                    {#if issue.sprintName && (sprintFilter === "all" || sprintFilter === "none")}<span class="faint">{issue.sprintName}</span>{/if}
                    {#if issue.assignee}<span class="faint">· {issue.assignee}</span>{/if}
                    {#if issue.dueDate}<span class="faint">· fällig {issue.dueDate}</span>{/if}
                    {#if issue.source === "manual" || issue.source === "tracked"}<span class="faint">· nicht im Sprint</span>{/if}
                  </div>
                </div>
                <div class="budget">
                  {#if pct != null}
                    <div class="bar" title="{formatDuration(tracked(issue.key))} von {formatDuration(issue.originalEstimateS ?? 0)} h geschätzt">
                      <div class="fill" class:warn={pct > 80} class:over={pct > 100} style="width:{Math.min(100, pct)}%"></div>
                    </div>
                    <span class="mono small">{formatDuration(tracked(issue.key), store.timeFormat)} / {formatDuration(issue.originalEstimateS ?? 0, store.timeFormat)}</span>
                  {:else}
                    <span class="mono small muted">{tracked(issue.key) ? formatDuration(tracked(issue.key), store.timeFormat) + " h" : ""}</span>
                  {/if}
                </div>
                <button class="btn icon" class:running={!!running} type="button" onclick={() => toggle(issue)} title={running ? "Timer stoppen" : "Timer starten"} aria-label={running ? "Timer stoppen" : "Timer starten"}>
                  <Icon name={running ? "stop" : "play"} size={16} />
                </button>
              </div>
            {/each}
          </div>
        </section>
      {/each}
      {#if groups.length === 0}
        <div class="empty card">
          <p class="muted">Keine Tickets passen zu den Filtern.</p>
        </div>
      {/if}
      <div class="add row">
        <Icon name="plus" size={15} />
        <input type="text" placeholder="Ticket per Key hinzufügen (z. B. ABC-123) …" bind:value={addKey} onkeydown={(e) => e.key === "Enter" && addIssue()} />
        <button class="btn sm" type="button" onclick={addIssue} disabled={adding || !addKey.trim()}>Hinzufügen</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .small {
    font-size: 12px;
  }
  .toolbar {
    display: flex;
    gap: 8px;
    padding: 0 20px 12px;
    flex-wrap: wrap;
    align-items: center;
  }
  .search {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 8px;
    color: var(--text-3);
    min-width: 200px;
  }
  .search input {
    border: none;
    background: transparent;
    flex: 1;
    box-shadow: none !important;
    color: var(--text);
    padding: 6px 0;
  }
  .toolbar select {
    max-width: 180px;
  }
  .btn.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .group {
    margin-bottom: 18px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
  }
  .rows {
    overflow: hidden;
  }
  .issue {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 6px;
    border-bottom: 1px solid var(--border);
    border-left: 3px solid transparent;
  }
  .issue:last-child {
    border-bottom: none;
  }
  .issue:hover {
    background: var(--surface-2);
  }
  .issue.running {
    border-left-color: var(--running);
    background: color-mix(in srgb, var(--running-soft) 50%, var(--surface));
  }
  .issue.child {
    padding-left: 34px;
  }
  .star {
    border: none;
    background: none;
    color: var(--border);
    cursor: pointer;
    padding: 4px;
    display: inline-flex;
  }
  .star:hover {
    color: var(--warning);
  }
  .star.on {
    color: var(--warning);
  }
  .type {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    color: #fff;
    flex-shrink: 0;
  }
  .type.bug {
    background: #e5484d;
  }
  .type.story {
    background: #30a46c;
  }
  .type.task {
    background: #3e63dd;
  }
  .type.subtask {
    background: #8e8c99;
  }
  .type.epic {
    background: #8e4ec6;
  }
  .main {
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
    flex-shrink: 0;
  }
  .key:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .summary {
    font-weight: 500;
  }
  .meta {
    font-size: 12px;
    color: var(--text-2);
    gap: 6px;
  }
  .budget {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
    width: 150px;
    flex-shrink: 0;
  }
  .bar {
    width: 100%;
    height: 5px;
    background: var(--surface-3);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
  }
  .fill.warn {
    background: var(--warning);
  }
  .fill.over {
    background: var(--danger);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 36px 24px;
    gap: 8px;
    color: var(--text-2);
    margin-top: 10px;
  }
  .empty h3 {
    margin: 4px 0 0;
    color: var(--text);
  }
  .empty p {
    margin: 0 0 8px;
    max-width: 380px;
  }
  .add {
    margin-top: 8px;
    color: var(--text-3);
  }
  .add input {
    flex: 1;
    max-width: 420px;
  }
</style>
