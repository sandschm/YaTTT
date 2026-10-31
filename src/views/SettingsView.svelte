<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type AppInfo, type AuthMethod, type IssueType, type Project, type ProjectFilter, type Settings, type Sprint } from "../lib/api";
  import { t, weekdayShort, type Key } from "../lib/i18n.svelte";
  import { formatDate, toDateKey, addDays } from "../lib/format";
  import Icon from "../components/Icon.svelte";

  type Tab = "connection" | "selection" | "filter" | "time" | "behavior" | "tempo" | "info";
  // Browser mock only: ?tab=selection opens a settings tab directly (used for screenshots).
  const requestedTab = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window) ? new URLSearchParams(location.search).get("tab") : null;
  let tab = $state<Tab>((["connection", "selection", "filter", "time", "behavior", "tempo", "info"].includes(requestedTab ?? "") ? requestedTab : "connection") as Tab);
  const tabs: { id: Tab; label: Key }[] = [
    { id: "connection", label: "settings.tab.connection" },
    { id: "selection", label: "settings.tab.selection" },
    { id: "filter", label: "settings.tab.filter" },
    { id: "time", label: "settings.tab.time" },
    { id: "behavior", label: "settings.tab.behavior" },
    { id: "tempo", label: "settings.tab.tempo" },
    { id: "info", label: "settings.tab.info" },
  ];

  // ---- draft handling: edits go to a local copy and are saved debounced
  let draft = $state<Settings | null>(null);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (store.settings && !draft) draft = structuredClone($state.snapshot(store.settings));
  });
  $effect(() => {
    // external changes (e.g. project selection via API) replace the draft when not dirty
    const s = store.settings;
    if (s && !dirty) draft = structuredClone($state.snapshot(s));
  });
  let dirty = $state(false);

  function change() {
    dirty = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, 500);
  }
  async function saveNow() {
    if (!draft) return;
    try {
      const saved = await api.saveSettings($state.snapshot(draft));
      store.settings = saved;
      dirty = false;
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  // ---- connection
  let username = $state("");
  let password = $state("");
  let token = $state("");
  let email = $state("");
  let busy = $state(false);
  let authError = $state("");
  const auth = $derived(store.auth);
  const authMethods: [AuthMethod, Key][] = [
    ["browserSession", "settings.conn.browser"],
    ["password", "settings.conn.password"],
    ["personalAccessToken", "settings.conn.pat"],
    ["apiToken", "settings.conn.apiToken"],
  ];
  const methodHelp: Record<AuthMethod, Key> = {
    browserSession: "settings.conn.help.browserSession",
    password: "settings.conn.help.password",
    personalAccessToken: "settings.conn.help.personalAccessToken",
    apiToken: "settings.conn.help.apiToken",
  };
  function viaLabel(method: string | null): string {
    switch (method) {
      case "cookies":
        return t("settings.conn.viaCookies");
      case "basic":
        return t("settings.conn.viaBasic");
      case "bearer":
        return t("settings.conn.viaBearer");
      default:
        return t("settings.conn.viaApiToken");
    }
  }

  async function connect() {
    if (!draft) return;
    authError = "";
    busy = true;
    try {
      await saveNow();
      switch (draft.jira.authMethod) {
        case "browserSession":
          await api.startBrowserLogin();
          break;
        case "password":
          await api.loginWithPassword(username, password);
          password = "";
          break;
        case "personalAccessToken":
          await api.loginWithToken("pat", null, token);
          token = "";
          break;
        case "apiToken":
          await api.loginWithToken("apiToken", email, token);
          token = "";
          break;
      }
      store.auth = await api.getAuthStatus();
      store.toast(t("settings.conn.signedInAs", { name: store.auth.user?.displayName ?? "?" }), "success");
      await store.reloadIssues();
    } catch (e) {
      authError = String(e);
    } finally {
      busy = false;
    }
  }

  async function disconnect() {
    await api.logout();
    store.auth = await api.getAuthStatus();
  }

  // ---- projects & sprints
  let projectQuery = $state("");
  let projectResults = $state<Project[]>([]);
  let projectsLoading = $state(false);
  let sprintQuery = $state("");
  let includeClosed = $state(false);
  let sprintResults = $state<Sprint[]>([]);
  let sprintsLoading = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let sprintTimer: ReturnType<typeof setTimeout> | undefined;

  const selectedProjects = $derived(store.projects.filter((p) => store.settings?.selection.projectKeys.includes(p.key)));
  const selectedSprints = $derived((store.snapshot?.sprints ?? []).filter((s) => store.settings?.selection.sprintIds.includes(s.id)));

  async function searchProjects() {
    if (!store.connected) return;
    projectsLoading = true;
    try {
      projectResults = await api.searchProjects(projectQuery);
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      projectsLoading = false;
    }
  }
  function onProjectQuery() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(searchProjects, 350);
  }
  async function toggleProject(p: Project) {
    const current = selectedProjects.filter((x) => x.key !== p.key);
    const next = p.selected || selectedProjects.some((x) => x.key === p.key) ? current : [...current, { ...p, selected: true }];
    try {
      store.settings = await api.setProjectSelection(next);
      await store.reloadIssues();
      projectResults = projectResults.map((r) => ({ ...r, selected: next.some((n) => n.key === r.key) }));
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  async function setColor(key: string, color: string) {
    await api.setProjectColor(key, color);
    store.settings = await api.getSettings();
    await store.reloadIssues();
  }

  // ---- filters per project: labels and/or components (projects shared by several teams)
  type FilterKind = "labels" | "components";
  const filterKinds: [FilterKind, Key][] = [
    ["components", "settings.sel.components"],
    ["labels", "settings.sel.labels"],
  ];
  let filterQuery = $state<Record<string, string>>({});
  let filterSuggestions = $state<Record<string, string[]>>({});
  let filterFocus = $state<Record<string, boolean>>({});
  let componentCache = $state<Record<string, string[]>>({});
  const filterTimers: Record<string, ReturnType<typeof setTimeout>> = {};
  const fid = (kind: FilterKind, key: string) => `${kind}:${key}`;

  function projectFilter(key: string): ProjectFilter {
    return draft?.selection.projectFilters[key] ?? { labels: [], components: [], matchAll: true };
  }
  function chosenValues(kind: FilterKind, key: string): string[] {
    return projectFilter(key)[kind];
  }
  function localValues(kind: FilterKind, key: string): string[] {
    return [...new Set(store.issues.filter((i) => i.projectKey === key).flatMap((i) => i[kind]))];
  }
  function setFilter(key: string, patch: Partial<ProjectFilter>) {
    if (!draft) return;
    const current = { ...projectFilter(key), ...patch };
    const next = { ...draft.selection.projectFilters };
    if (current.labels.length || current.components.length) next[key] = current;
    else delete next[key];
    draft.selection.projectFilters = next;
    change();
  }
  function listPatch(kind: FilterKind, list: string[]): Partial<ProjectFilter> {
    return kind === "labels" ? { labels: list } : { components: list };
  }
  function addValue(kind: FilterKind, key: string, value: string) {
    const v = value.trim();
    if (!v) return;
    const current = chosenValues(kind, key);
    if (!current.includes(v)) setFilter(key, listPatch(kind, [...current, v]));
    filterQuery = { ...filterQuery, [fid(kind, key)]: "" };
    searchValues(kind, key);
  }
  function removeValue(kind: FilterKind, key: string, value: string) {
    setFilter(key, listPatch(kind, chosenValues(kind, key).filter((x) => x !== value)));
  }
  function toggleValue(kind: FilterKind, key: string, value: string) {
    if (chosenValues(kind, key).includes(value)) removeValue(kind, key, value);
    else addValue(kind, key, value);
  }
  function onFilterQuery(kind: FilterKind, key: string, value: string) {
    const id = fid(kind, key);
    filterQuery = { ...filterQuery, [id]: value };
    clearTimeout(filterTimers[id]);
    filterTimers[id] = setTimeout(() => searchValues(kind, key), 300);
  }
  function onFilterKey(e: KeyboardEvent, kind: FilterKind, key: string) {
    if (e.key === "Enter") {
      e.preventDefault();
      addValue(kind, key, filterQuery[fid(kind, key)] ?? "");
    }
  }
  function onFilterFocus(kind: FilterKind, key: string) {
    filterFocus = { ...filterFocus, [fid(kind, key)]: true };
    searchValues(kind, key);
  }
  function onFilterBlur(kind: FilterKind, key: string) {
    // delayed so that a click on a suggestion still registers
    setTimeout(() => (filterFocus = { ...filterFocus, [fid(kind, key)]: false }), 200);
  }
  async function searchValues(kind: FilterKind, key: string) {
    const id = fid(kind, key);
    const q = (filterQuery[id] ?? "").trim().toLowerCase();
    let pool = localValues(kind, key);
    if (store.connected) {
      try {
        if (kind === "components") {
          // components are per project: load the complete list once
          if (!componentCache[key]) componentCache = { ...componentCache, [key]: await api.listComponents(key) };
          pool = [...pool, ...componentCache[key]];
        } else if (q) {
          pool = [...pool, ...(await api.searchLabels(q))];
        }
      } catch {
        // suggestions are optional; typing + Enter always works
      }
    }
    const matches = [...new Set(pool)]
      .filter((v) => !q || v.toLowerCase().includes(q))
      .sort((a, b) => a.localeCompare(b))
      .slice(0, 25);
    filterSuggestions = { ...filterSuggestions, [id]: matches };
  }

  async function searchSprints() {
    if (!store.connected) return;
    sprintsLoading = true;
    try {
      sprintResults = await api.searchSprints(sprintQuery, includeClosed);
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      sprintsLoading = false;
    }
  }
  function onSprintQuery() {
    clearTimeout(sprintTimer);
    sprintTimer = setTimeout(searchSprints, 350);
  }
  async function toggleSprint(s: Sprint) {
    const isSelected = selectedSprints.some((x) => x.id === s.id);
    const next = isSelected ? selectedSprints.filter((x) => x.id !== s.id) : [...selectedSprints, { ...s, selected: true }];
    try {
      store.settings = await api.setSprintSelection(next);
      await store.reloadIssues();
      sprintResults = sprintResults.map((r) => ({ ...r, selected: next.some((n) => n.id === r.id) }));
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  function sprintState(state: string): string {
    return state === "active" ? t("settings.sel.sprintActive") : state === "future" ? t("settings.sel.sprintFuture") : t("settings.sel.sprintClosed");
  }

  $effect(() => {
    if (tab === "selection" && store.connected) {
      searchProjects();
      searchSprints();
    }
  });

  // ---- issue types
  let issueTypes = $state<IssueType[]>([]);
  $effect(() => {
    if (tab === "filter") {
      if (store.connected) api.listIssueTypes().then((ty) => (issueTypes = ty)).catch(() => {});
      if (!issueTypes.length) issueTypes = [...new Set(store.issues.map((i) => i.issueType))].map((n, i) => ({ id: String(i), name: n, subtask: false, iconUrl: null }));
    }
  });
  const allTypeNames = $derived.by(() => {
    const names = new Set<string>(issueTypes.map((ty) => ty.name));
    for (const n of draft?.selection.excludedIssueTypes ?? []) names.add(n);
    return [...names].sort();
  });
  function toggleType(name: string) {
    if (!draft) return;
    const list = draft.selection.excludedIssueTypes;
    draft.selection.excludedIssueTypes = list.includes(name) ? list.filter((x) => x !== name) : [...list, name];
    change();
  }

  // ---- tempo
  const tempoModes: [Settings["tempo"]["mode"], Key, Key][] = [
    ["jiraWorklog", "settings.tempo.jiraWorklog", "settings.tempo.jiraWorklogHelp"],
    ["tempoDataCenter", "settings.tempo.tempoDC", "settings.tempo.tempoDCHelp"],
    ["tempoCloud", "settings.tempo.tempoCloud", "settings.tempo.tempoCloudHelp"],
  ];
  let tempoToken = $state("");
  let tempoBusy = $state(false);
  async function saveTempoToken(clear = false) {
    tempoBusy = true;
    try {
      await api.setTempoCloudToken(clear ? null : tempoToken);
      tempoToken = "";
      store.settings = await api.getSettings();
      store.toast(clear ? t("settings.tempo.tokenRemoved") : t("settings.tempo.tokenSaved"), "success");
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      tempoBusy = false;
    }
  }

  // ---- info / import
  let info = $state<AppInfo | null>(null);
  let importFrom = $state(toDateKey(addDays(new Date(), -30)));
  let importTo = $state(toDateKey(new Date()));
  let importing = $state(false);
  $effect(() => {
    if (tab === "info" && !info) api.getAppInfo().then((i) => (info = i));
  });
  async function runImport() {
    importing = true;
    try {
      const n = await api.importWorklogs(importFrom, importTo);
      store.toast(t("settings.info.imported", { n }), "success");
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      importing = false;
    }
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>{t("settings.title")}</h1>
    <div class="grow"></div>
    {#if dirty}<span class="faint">{t("settings.saving")}</span>{/if}
  </div>
  <div class="layout">
    <nav class="tabs">
      {#each tabs as tb (tb.id)}
        <button class:on={tab === tb.id} type="button" onclick={() => (tab = tb.id)}>{t(tb.label)}</button>
      {/each}
    </nav>
    <div class="pane">
      {#if !draft}
        <p class="faint">{t("app.loading")}</p>
      {:else if tab === "connection"}
        <section class="card block">
          <h3>{t("settings.conn.title")}</h3>
          <div class="field">
            <label for="jira-url">{t("settings.conn.url")}</label>
            <input id="jira-url" type="url" placeholder={t("settings.conn.urlPlaceholder")} bind:value={draft.jira.baseUrl} oninput={change} />
            <span class="hint">{t("settings.conn.urlHint")}</span>
          </div>
          <div class="field">
            <span class="label-text">{t("settings.conn.method")}</span>
            {#each authMethods as [id, label] (id)}
              <label class="radio">
                <input type="radio" name="auth" value={id} bind:group={draft.jira.authMethod} onchange={change} />
                <span>{t(label)}</span>
              </label>
            {/each}
            <span class="hint">{t(methodHelp[draft.jira.authMethod])}</span>
          </div>
          {#if draft.jira.authMethod === "password"}
            <div class="two">
              <div class="field"><label for="u">{t("settings.conn.username")}</label><input id="u" type="text" bind:value={username} autocomplete="username" /></div>
              <div class="field"><label for="p">{t("settings.conn.passwordLabel")}</label><input id="p" type="password" bind:value={password} autocomplete="current-password" onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {:else if draft.jira.authMethod === "personalAccessToken"}
            <div class="field"><label for="t">{t("settings.conn.patLabel")}</label><input id="t" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
          {:else if draft.jira.authMethod === "apiToken"}
            <div class="two">
              <div class="field"><label for="e">{t("settings.conn.email")}</label><input id="e" type="email" bind:value={email} /></div>
              <div class="field"><label for="t2">{t("settings.conn.apiTokenLabel")}</label><input id="t2" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {/if}
          {#if authError}<div class="error"><Icon name="warning" size={14} /> {authError}</div>{/if}
          <div class="row wrap">
            <button class="btn primary" type="button" onclick={connect} disabled={busy || !draft.jira.baseUrl}>
              {#if busy}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
              {auth.authenticated ? t("settings.conn.signInAgain") : t("settings.conn.signIn")}
            </button>
            {#if store.loginWindowOpen}
              <button class="btn" type="button" onclick={() => api.cancelBrowserLogin()}>{t("settings.conn.closeLoginWindow")}</button>
              <span class="faint">{t("settings.conn.loginHint")}</span>
            {/if}
          </div>
          {#if store.loginWindowOpen && store.loginProgress}
            <div class="progress"><span class="spin"><Icon name="refresh" size={13} /></span> {store.loginProgress}</div>
          {/if}
          <div class="row wrap" style="display:none">
            {#if auth.authenticated}
              <button class="btn" type="button" onclick={disconnect}>{t("settings.conn.signOut")}</button>
            {/if}
          </div>
        </section>
        <section class="card block status">
          <h3>{t("settings.conn.status")}</h3>
          {#if auth.authenticated && auth.user}
            <div class="row"><span class="dot" style="background: var(--success)"></span><strong>{auth.user.displayName}</strong><span class="muted">{auth.user.email ?? ""}</span></div>
            {#if auth.server}
              <div class="muted">{auth.server.serverTitle ?? auth.server.baseUrl} · Jira {auth.server.deployment === "cloud" ? t("settings.conn.cloud") : t("settings.conn.dataCenter")} {auth.server.version}</div>
            {/if}
            <div class="muted">{t("settings.conn.via", { method: viaLabel(auth.method) })}</div>
          {:else}
            <div class="row"><span class="dot" style="background: var(--warning)"></span><span>{t("settings.conn.notSignedIn")}</span></div>
            {#if auth.message}<div class="muted">{auth.message}</div>{/if}
          {/if}
        </section>
      {:else if tab === "selection"}
        {#if !store.connected}
          <p class="muted">{t("settings.sel.connectFirst")}</p>
        {:else}
          <section class="card block">
            <h3>{t("settings.sel.projects")}</h3>
            <p class="hint">{t("settings.sel.projectsHint")}</p>
            {#if selectedProjects.length}
              <div class="chips">
                {#each selectedProjects as p (p.key)}
                  <span class="chip" style="--c:{store.color(p.key)}">
                    <input type="color" value={store.color(p.key)} onchange={(e) => setColor(p.key, (e.target as HTMLInputElement).value)} title={t("settings.sel.color")} />
                    <span class="mono">{p.key}</span>
                    <span class="ellipsis">{p.name}</span>
                    <button type="button" onclick={() => toggleProject({ ...p, selected: true })} aria-label={t("common.remove")}><Icon name="x" size={12} /></button>
                  </span>
                {/each}
              </div>
            {/if}
            <div class="search row">
              <Icon name="search" size={15} />
              <input type="search" placeholder={t("settings.sel.searchProjects")} bind:value={projectQuery} oninput={onProjectQuery} />
              {#if projectsLoading}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
            </div>
            <ul class="list">
              {#each projectResults as p (p.key)}
                <li>
                  <label class="row">
                    <input type="checkbox" checked={selectedProjects.some((x) => x.key === p.key)} onchange={() => toggleProject({ ...p, selected: selectedProjects.some((x) => x.key === p.key) })} />
                    <span class="mono key">{p.key}</span>
                    <span class="ellipsis grow">{p.name}</span>
                  </label>
                </li>
              {/each}
              {#if !projectResults.length && !projectsLoading}<li class="faint">{t("settings.sel.noProjects")}</li>{/if}
            </ul>
          </section>
          {#if selectedProjects.length && draft}
            <section class="card block">
              <h3>{t("settings.sel.filterTitle")}</h3>
              <p class="hint">{t("settings.sel.filterHint")}</p>
              {#each selectedProjects as p (p.key)}
                {@const filter = projectFilter(p.key)}
                <div class="filter-row">
                  <div class="label-project">
                    <span class="dot" style="background:{store.color(p.key)}"></span>
                    <span class="mono key">{p.key}</span>
                    <span class="ellipsis faint">{p.name}</span>
                  </div>
                  <div class="grow">
                    {#each filterKinds as [kind, title] (kind)}
                      {@const id = fid(kind, p.key)}
                      {@const picked = chosenValues(kind, p.key)}
                      {@const suggestions = (filterSuggestions[id] ?? []).filter((v) => !picked.includes(v))}
                      <div class="filter-kind">
                        <span class="kind-title">{t(title)}</span>
                        <div class="grow">
                          {#if picked.length}
                            <div class="chips">
                              {#each picked as v (v)}
                                <span class="chip">
                                  <span>{v}</span>
                                  <button type="button" onclick={() => removeValue(kind, p.key, v)} aria-label={t("common.remove")}><Icon name="x" size={12} /></button>
                                </span>
                              {/each}
                            </div>
                          {/if}
                          <div class="search row">
                            <Icon name="search" size={15} />
                            <input
                              type="search"
                              placeholder={kind === "components" ? t("settings.sel.componentPlaceholder", { key: p.key }) : t("settings.sel.labelPlaceholder", { key: p.key })}
                              value={filterQuery[id] ?? ""}
                              oninput={(e) => onFilterQuery(kind, p.key, (e.target as HTMLInputElement).value)}
                              onkeydown={(e) => onFilterKey(e, kind, p.key)}
                              onfocus={() => onFilterFocus(kind, p.key)}
                              onblur={() => onFilterBlur(kind, p.key)}
                            />
                          </div>
                          {#if (filterFocus[id] || (filterQuery[id] ?? "").trim()) && suggestions.length}
                            <ul class="list">
                              {#each suggestions as v (v)}
                                <li>
                                  <label class="row">
                                    <input type="checkbox" checked={false} onchange={() => toggleValue(kind, p.key, v)} />
                                    <span class="grow">{v}</span>
                                  </label>
                                </li>
                              {/each}
                            </ul>
                          {/if}
                        </div>
                      </div>
                    {/each}
                    {#if filter.labels.length && filter.components.length}
                      <label class="row match-mode">
                        <span>{t("settings.sel.matchPrefix")}</span>
                        <select value={filter.matchAll ? "all" : "any"} onchange={(e) => setFilter(p.key, { matchAll: (e.target as HTMLSelectElement).value === "all" })}>
                          <option value="all">{t("settings.sel.matchAll")}</option>
                          <option value="any">{t("settings.sel.matchAny")}</option>
                        </select>
                        {#if t("settings.sel.matchSuffix")}<span>{t("settings.sel.matchSuffix")}</span>{/if}
                      </label>
                    {/if}
                  </div>
                </div>
              {/each}
            </section>
          {/if}
          <section class="card block">
            <h3>{t("settings.sel.sprintsTitle")}</h3>
            <p class="hint">{t("settings.sel.sprintsHint")}</p>
            {#if selectedSprints.length}
              <div class="chips">
                {#each selectedSprints as s (s.id)}
                  <span class="chip">
                    <span>{s.name}</span>
                    <span class="badge" class:success={s.state === "active"}>{sprintState(s.state)}</span>
                    <button type="button" onclick={() => toggleSprint(s)} aria-label={t("common.remove")}><Icon name="x" size={12} /></button>
                  </span>
                {/each}
              </div>
            {/if}
            <div class="row wrap">
              <div class="search row grow">
                <Icon name="search" size={15} />
                <input type="search" placeholder={t("settings.sel.searchSprints")} bind:value={sprintQuery} oninput={onSprintQuery} disabled={!selectedProjects.length} />
                {#if sprintsLoading}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
              </div>
              <label class="check" style="margin:0"><input type="checkbox" bind:checked={includeClosed} onchange={searchSprints} /><span>{t("settings.sel.includeClosed")}</span></label>
            </div>
            <ul class="list">
              {#each sprintResults as s (s.id)}
                <li>
                  <label class="row">
                    <input type="checkbox" checked={selectedSprints.some((x) => x.id === s.id)} onchange={() => toggleSprint(s)} />
                    <span class="grow">{s.name}</span>
                    <span class="badge" class:success={s.state === "active"}>{sprintState(s.state)}</span>
                    {#if s.startDate}<span class="faint small">{formatDate(s.startDate, { day: "2-digit", month: "2-digit" })}{s.endDate ? " – " + formatDate(s.endDate, { day: "2-digit", month: "2-digit" }) : ""}</span>{/if}
                  </label>
                </li>
              {/each}
              {#if !sprintResults.length && !sprintsLoading}
                <li class="faint">{selectedProjects.length ? t("settings.sel.noSprints") : t("settings.sel.selectProjectsFirst")}</li>
              {/if}
            </ul>
          </section>
          <section class="card block">
            <h3>{t("settings.sel.selectionTitle")}</h3>
            <label class="check"><input type="checkbox" bind:checked={draft.selection.includeOpenSprints} onchange={change} /><span>{t("settings.sel.includeOpenSprints")}</span></label>
            <label class="check"><input type="checkbox" bind:checked={draft.selection.onlyAssignedToMe} onchange={change} /><span>{t("settings.sel.onlyMine")}</span></label>
            <label class="check"><input type="checkbox" bind:checked={draft.selection.hideDone} onchange={change} /><span>{t("settings.sel.hideDone")} <span class="hint">{t("settings.sel.hideDoneHint")}</span></span></label>
            <div class="field">
              <label for="jql">{t("settings.sel.jql")}</label>
              <input id="jql" type="text" placeholder={t("settings.sel.jqlPlaceholder")} bind:value={draft.selection.extraJql} oninput={change} />
              <p class="hint">{t("settings.sel.jqlHint")}</p>
            </div>
          </section>
        {/if}
      {:else if tab === "filter"}
        <section class="card block">
          <h3>{t("settings.types.title")}</h3>
          <p class="hint">{t("settings.types.hint")}</p>
          {#each allTypeNames as name (name)}
            <label class="check"><input type="checkbox" checked={draft.selection.excludedIssueTypes.includes(name)} onchange={() => toggleType(name)} /><span>{name}</span></label>
          {/each}
          {#if !allTypeNames.length}<p class="faint">{t("settings.types.none")}</p>{/if}
        </section>
      {:else if tab === "time"}
        <section class="card block">
          <h3>{t("settings.time.work")}</h3>
          <div class="weekdays">
            {#each [0, 1, 2, 3, 4, 5, 6] as i (i)}
              <div class="field">
                <label for="wd{i}">{weekdayShort(i)}</label>
                <input id="wd{i}" type="number" min="0" max="24" step="0.5" bind:value={draft.work.hoursPerWeekday[i]} onchange={change} />
              </div>
            {/each}
          </div>
          <div class="two">
            <div class="field">
              <label for="fdow">{t("settings.time.firstDay")}</label>
              <select id="fdow" bind:value={draft.work.firstDayOfWeek} onchange={change}>
                <option value={0}>{t("settings.time.monday")}</option>
                <option value={6}>{t("settings.time.sunday")}</option>
              </select>
            </div>
            <div class="field">
              <label for="tfmt">{t("settings.time.format")}</label>
              <select id="tfmt" bind:value={draft.work.timeFormat} onchange={change}>
                <option value="hoursMinutes">{t("settings.time.formatHM")}</option>
                <option value="decimal">{t("settings.time.formatDecimal")}</option>
              </select>
            </div>
          </div>
        </section>
        <section class="card block">
          <h3>{t("settings.time.timer")}</h3>
          <div class="two">
            <div class="field">
              <label for="idle">{t("settings.time.idle")}</label>
              <input id="idle" type="number" min="0" max="120" bind:value={draft.timer.idleTimeoutMinutes} onchange={change} />
              <span class="hint">{t("settings.time.idleHint")}</span>
            </div>
            <div class="field">
              <label for="short">{t("settings.time.short")}</label>
              <input id="short" type="number" min="0" max="3600" bind:value={draft.timer.ignoreShortEntriesSeconds} onchange={change} />
            </div>
            <div class="field">
              <label for="long">{t("settings.time.long")}</label>
              <input id="long" type="number" min="0" max="48" bind:value={draft.timer.longRunningWarningHours} onchange={change} />
            </div>
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.timer.allowConcurrent} onchange={change} /><span>{t("settings.time.concurrent")} <span class="hint">{t("settings.time.concurrentHint")}</span></span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.timer.defaultBillable} onchange={change} /><span>{t("settings.time.billable")}</span></label>
        </section>
        <section class="card block">
          <h3>{t("settings.time.reminders")}</h3>
          <div class="two">
            <div class="field">
              <label for="wstart">{t("settings.time.workdayStart")}</label>
              <input id="wstart" type="time" bind:value={draft.work.workdayStart} onchange={change} />
            </div>
            <div class="field">
              <label for="brk">{t("settings.time.break")}</label>
              <input id="brk" type="number" min="0" max="600" step="15" bind:value={draft.work.breakReminderAfterMinutes} onchange={change} />
            </div>
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.work.dailyGoalNotification} onchange={change} /><span>{t("settings.time.goal")}</span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.work.unbookedReminder} onchange={change} /><span>{t("settings.time.unbooked")}</span></label>
        </section>
      {:else if tab === "behavior"}
        <section class="card block">
          <h3>{t("settings.behavior.app")}</h3>
          <label class="check"><input type="checkbox" bind:checked={draft.behavior.closeToTray} onchange={change} /><span>{t("settings.behavior.closeToTray")} <span class="hint">{t("settings.behavior.closeToTrayHint")}</span></span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.behavior.autostart} onchange={change} /><span>{t("settings.behavior.autostart")}</span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.behavior.notifications} onchange={change} /><span>{t("settings.behavior.notifications")}</span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.behavior.showTimerInTray} onchange={change} /><span>{t("settings.behavior.trayTimer")}</span></label>
          <div class="two">
            <div class="field">
              <label for="theme">{t("settings.behavior.theme")}</label>
              <select id="theme" bind:value={draft.behavior.theme} onchange={change}>
                <option value="system">{t("settings.behavior.themeSystem")}</option>
                <option value="light">{t("settings.behavior.themeLight")}</option>
                <option value="dark">{t("settings.behavior.themeDark")}</option>
              </select>
            </div>
            <div class="field">
              <label for="language">{t("settings.behavior.language")}</label>
              <select id="language" bind:value={draft.behavior.language} onchange={change}>
                <option value="system">{t("settings.behavior.langSystem")}</option>
                <option value="de">{t("settings.behavior.langDe")}</option>
                <option value="en">{t("settings.behavior.langEn")}</option>
              </select>
              <span class="hint">{t("settings.behavior.languageHint")}</span>
            </div>
          </div>
        </section>
        <section class="card block">
          <h3>{t("settings.behavior.shortcuts")}</h3>
          <div class="two">
            <div class="field">
              <label for="sc1">{t("settings.behavior.toggleTimer")}</label>
              <input id="sc1" type="text" bind:value={draft.behavior.shortcutToggleTimer} onchange={change} placeholder="CommandOrControl+Shift+T" />
            </div>
            <div class="field">
              <label for="sc2">{t("settings.behavior.showWindow")}</label>
              <input id="sc2" type="text" bind:value={draft.behavior.shortcutShowWindow} onchange={change} placeholder="CommandOrControl+Shift+Y" />
            </div>
          </div>
          <span class="hint">{t("settings.behavior.shortcutHint")}</span>
        </section>
        <section class="card block">
          <h3>{t("settings.behavior.refresh")}</h3>
          <label class="check"><input type="checkbox" bind:checked={draft.refresh.auto} onchange={change} /><span>{t("settings.behavior.autoRefresh")}</span></label>
          <div class="field">
            <label for="interval">{t("settings.behavior.interval")}</label>
            <input id="interval" type="number" min="1" max="240" bind:value={draft.refresh.intervalMinutes} onchange={change} />
          </div>
        </section>
      {:else if tab === "tempo"}
        <section class="card block">
          <h3>{t("settings.tempo.title")}</h3>
          {#each tempoModes as [id, label, help] (id)}
            <label class="radio block-radio">
              <input type="radio" name="tempo" value={id} bind:group={draft.tempo.mode} onchange={change} />
              <span><strong>{t(label)}</strong><span class="hint">{t(help)}</span></span>
            </label>
          {/each}
          {#if draft.tempo.mode === "tempoCloud"}
            <div class="field">
              <label for="tbase">{t("settings.tempo.apiBase")}</label>
              <input id="tbase" type="url" bind:value={draft.tempo.cloudApiBase} oninput={change} />
            </div>
            <div class="field">
              <label for="ttoken">{t("settings.tempo.token")} {draft.tempo.cloudTokenSet ? t("settings.tempo.tokenSet") : ""}</label>
              <div class="row">
                <input id="ttoken" type="password" bind:value={tempoToken} placeholder={draft.tempo.cloudTokenSet ? "••••••••" : t("settings.tempo.tokenPlaceholder")} />
                <button class="btn" type="button" onclick={() => saveTempoToken(false)} disabled={tempoBusy || !tempoToken}>{t("settings.tempo.checkSave")}</button>
                {#if draft.tempo.cloudTokenSet}<button class="btn danger" type="button" onclick={() => saveTempoToken(true)} disabled={tempoBusy}>{t("common.remove")}</button>{/if}
              </div>
            </div>
          {/if}
        </section>
        <section class="card block">
          <h3>{t("settings.tempo.rounding")}</h3>
          <p class="hint">{t("settings.tempo.roundingHint")}</p>
          <div class="two">
            <div class="field">
              <label for="rmin">{t("settings.tempo.roundTo")}</label>
              <select id="rmin" bind:value={draft.timer.rounding.minutes} onchange={change}>
                <option value={0}>{t("settings.tempo.noRounding")}</option>
                {#each [5, 6, 10, 15, 30] as m (m)}
                  <option value={m}>{t("settings.tempo.minutes", { n: m })}</option>
                {/each}
              </select>
            </div>
            <div class="field">
              <label for="rmode">{t("settings.tempo.direction")}</label>
              <select id="rmode" bind:value={draft.timer.rounding.mode} onchange={change} disabled={!draft.timer.rounding.minutes}>
                <option value="nearest">{t("settings.tempo.nearest")}</option>
                <option value="up">{t("settings.tempo.up")}</option>
                <option value="down">{t("settings.tempo.down")}</option>
              </select>
            </div>
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.timer.bookOnStop} onchange={change} /><span>{t("settings.tempo.bookOnStop")}</span></label>
          <label class="check"><input type="checkbox" bind:checked={draft.timer.askNoteOnStop} onchange={change} /><span>{t("settings.tempo.askNote")}</span></label>
        </section>
      {:else if tab === "info"}
        <section class="card block">
          <h3>YaTTT</h3>
          {#if info}
            <dl>
              <dt>{t("settings.info.version")}</dt><dd>{info.version} ({info.platform})</dd>
              <dt>{t("settings.info.settings")}</dt><dd class="mono small">{info.configDir}</dd>
              <dt>{t("settings.info.database")}</dt><dd class="mono small">{info.dataDir}</dd>
              <dt>{t("settings.info.logs")}</dt><dd class="mono small">{info.logDir}</dd>
              <dt>{t("settings.info.credentials")}</dt>
              <dd>
                {#if info.secretBackend === "keyring"}
                  {t("settings.info.keyring")}
                {:else}
                  <span class="badge warning">{t("settings.info.noKeyring")}</span> {t("settings.info.fileKey")}
                {/if}
              </dd>
            </dl>
          {/if}
        </section>
        <section class="card block">
          <h3>{t("settings.info.importTitle")}</h3>
          <p class="hint">{t("settings.info.importHint")}</p>
          <div class="row wrap">
            <input type="date" bind:value={importFrom} />
            <span class="faint">{t("common.to")}</span>
            <input type="date" bind:value={importTo} />
            <button class="btn" type="button" onclick={runImport} disabled={importing || !store.connected}><Icon name="download" size={14} /> {t("settings.info.import")}</button>
          </div>
        </section>
      {/if}
    </div>
  </div>
</div>

<style>
  .small {
    font-size: 12px;
  }
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .tabs {
    width: 190px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 12px 0 20px;
  }
  .tabs button {
    text-align: left;
    border: none;
    background: transparent;
    padding: 7px 10px;
    border-radius: 7px;
    cursor: pointer;
    color: var(--text-2);
  }
  .tabs button:hover {
    background: var(--surface-2);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--accent);
    box-shadow: var(--shadow);
    font-weight: 600;
  }
  .pane {
    flex: 1;
    overflow: auto;
    padding: 0 20px 20px 8px;
    min-width: 0;
  }
  .block {
    padding: 16px 18px;
    margin-bottom: 12px;
    max-width: 760px;
  }
  .block h3 {
    margin: 0 0 10px;
    font-size: 14px;
  }
  .block p.hint,
  .hint {
    font-size: 12px;
    color: var(--text-3);
    margin: 0 0 10px;
  }
  .label-text {
    font-size: 12.5px;
    color: var(--text-2);
    font-weight: 500;
  }
  .radio {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 3px 0;
    cursor: pointer;
  }
  .block-radio {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .block-radio .hint {
    display: block;
    margin: 2px 0 0;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .weekdays {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 8px;
  }
  .weekdays .field {
    margin-bottom: 8px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-2);
    margin-top: 10px;
    padding: 8px 10px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .error {
    color: var(--danger);
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 4px 0 10px;
    font-size: 12.5px;
  }
  .wrap {
    flex-wrap: wrap;
  }
  .status .row {
    margin-bottom: 4px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }
  .filter-row {
    display: block;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .filter-row > .grow {
    min-width: 0;
  }
  .label-project {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    margin-bottom: 8px;
  }
  .label-project .key {
    font-weight: 600;
  }
  .label-project .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .filter-kind {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-bottom: 8px;
  }
  .filter-kind > .grow {
    min-width: 0;
  }
  .kind-title {
    width: 95px;
    flex-shrink: 0;
    padding-top: 6px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .match-mode {
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
    margin-top: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px 3px 6px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
    max-width: 320px;
  }
  .chip input[type="color"] {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
  }
  .chip button {
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--text-3);
    display: inline-flex;
    padding: 2px;
  }
  .chip button:hover {
    color: var(--danger);
  }
  .search {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 8px;
    color: var(--text-3);
  }
  .search input {
    border: none;
    background: transparent;
    flex: 1;
    min-width: 0;
    box-shadow: none !important;
    color: var(--text);
    padding: 6px 0;
  }
  .list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .list li {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .list li:last-child {
    border-bottom: none;
  }
  .list label {
    cursor: pointer;
  }
  .list .key {
    color: var(--text-2);
    min-width: 56px;
  }
  dl {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    user-select: text;
  }
</style>
