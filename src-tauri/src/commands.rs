//! Tauri commands: thin wrappers around `yattt-core` and the jobs module.

use crate::jobs::{self, emit};
use crate::state::{AppState, IdlePrompt, RefreshStatus, RestorePrompt};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, State};
use yattt_core::models::*;
use yattt_core::secrets;
use yattt_core::settings::{AuthMethod, Settings};
use yattt_core::stats::Stats;
use yattt_core::sync::SyncSummary;
use yattt_core::timer::{StartResult, StopResult, TimerEngine};

type S<'a> = State<'a, Arc<AppState>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ------------------------------------------------------------------ settings

#[tauri::command]
pub fn get_settings(state: S<'_>) -> Settings {
    state.settings()
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: S<'_>,
    mut settings: Settings,
) -> Result<Settings, String> {
    settings.jira.base_url =
        yattt_core::settings::normalize_jira_url(&settings.jira.base_url).map_err(err)?;
    settings.validate().map_err(err)?;
    settings.selection.tidy_project_filters();
    let old = state.settings();
    yattt_core::settings::save_settings(&state.paths, &settings).map_err(err)?;
    *state.settings.write().unwrap() = settings.clone();
    state
        .store
        .set_selected_projects(&settings.selection.project_keys)
        .map_err(err)?;
    state
        .store
        .set_selected_sprints(&settings.selection.sprint_ids)
        .map_err(err)?;
    for (key, color) in &settings.project_colors {
        let _ = state.store.set_project_color(key, Some(color));
    }
    let url_changed = old.jira_base() != settings.jira_base();
    if url_changed && state.is_authenticated() {
        state.clear_session(Some("Jira-URL geändert. Bitte erneut anmelden.".into()));
        emit(&app, "auth-changed", state.auth_status());
    }
    crate::apply_settings(&app, &state, &old, &settings);
    emit(&app, "settings-changed", settings.clone());
    if old.selection != settings.selection && state.is_authenticated() {
        let app2 = app.clone();
        let state2 = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            let _ = jobs::refresh_issues(&app2, &state2).await;
        });
    }
    crate::tray::refresh(&app, &state);
    Ok(settings)
}

// ---------------------------------------------------------------------- auth

#[tauri::command]
pub fn get_auth_status(state: S<'_>) -> AuthStatus {
    state.auth_status()
}

#[tauri::command]
pub async fn start_browser_login(app: AppHandle, state: S<'_>) -> Result<AuthStatus, String> {
    let status = crate::login::browser_login(app.clone(), state.inner().clone()).await?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = AuthMethod::BrowserSession;
        s.jira.username = status.user.as_ref().map(|u| u.display_name.clone());
        let _ = yattt_core::settings::save_settings(&state.paths, &s);
    }
    kick_refresh(&app, &state);
    Ok(status)
}

#[tauri::command]
pub fn cancel_browser_login(app: AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(crate::login::LOGIN_WINDOW) {
        let _ = w.close();
    }
}

#[tauri::command]
pub async fn login_with_password(
    app: AppHandle,
    state: S<'_>,
    username: String,
    password: String,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Bitte zuerst die Jira-URL eintragen".into());
    }
    // Validate against the session resource first: gives clear errors (wrong password, CAPTCHA).
    yattt_core::jira::JiraClient::login_with_password(&base, &username, &password)
        .await
        .map_err(err)?;
    let credential = Credential::Basic {
        username: username.clone(),
        password,
    };
    let status = jobs::install_client(&app, &state, credential.clone()).await?;
    secrets::save_credential(state.secrets.as_ref(), &base, &credential).map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = AuthMethod::Password;
        s.jira.username = Some(username);
        let _ = yattt_core::settings::save_settings(&state.paths, &s);
    }
    kick_refresh(&app, &state);
    Ok(status)
}

#[tauri::command]
pub async fn login_with_token(
    app: AppHandle,
    state: S<'_>,
    kind: String,
    user: Option<String>,
    token: String,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Bitte zuerst die Jira-URL eintragen".into());
    }
    let (credential, method) = match kind.as_str() {
        "pat" => (
            Credential::Bearer {
                token: token.trim().to_string(),
            },
            AuthMethod::PersonalAccessToken,
        ),
        "apiToken" => (
            Credential::ApiToken {
                email: user.clone().unwrap_or_default().trim().to_string(),
                token: token.trim().to_string(),
            },
            AuthMethod::ApiToken,
        ),
        other => return Err(format!("Unbekannte Anmeldeart {other}")),
    };
    let status = jobs::install_client(&app, &state, credential.clone()).await?;
    secrets::save_credential(state.secrets.as_ref(), &base, &credential).map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        s.jira.auth_method = method;
        s.jira.username = status
            .user
            .as_ref()
            .map(|u| u.display_name.clone())
            .or(user);
        let _ = yattt_core::settings::save_settings(&state.paths, &s);
    }
    kick_refresh(&app, &state);
    Ok(status)
}

#[tauri::command]
pub fn logout(app: AppHandle, state: S<'_>) -> Result<(), String> {
    crate::login::logout(&app, &state)
}

fn kick_refresh(app: &AppHandle, state: &Arc<AppState>) {
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        let _ = jobs::refresh_issues(&app, &state).await;
    });
}

// ------------------------------------------------------------- jira metadata

#[tauri::command]
pub async fn search_projects(state: S<'_>, query: String) -> Result<Vec<Project>, String> {
    let jira = state.jira()?;
    let settings = state.settings();
    let known: HashMap<String, Project> = state
        .store
        .list_projects()
        .map_err(err)?
        .into_iter()
        .map(|p| (p.key.clone(), p))
        .collect();
    let mut projects = jira.search_projects(&query).await.map_err(|e| {
        if matches!(e, yattt_core::Error::AuthExpired(_)) {
            state.mark_auth_expired();
        }
        e.to_string()
    })?;
    for p in &mut projects {
        p.selected = settings.selection.project_keys.contains(&p.key);
        p.color = settings
            .project_colors
            .get(&p.key)
            .cloned()
            .or_else(|| known.get(&p.key).and_then(|k| k.color.clone()));
    }
    Ok(projects)
}

#[tauri::command]
pub async fn search_sprints(
    state: S<'_>,
    query: String,
    include_closed: bool,
) -> Result<Vec<Sprint>, String> {
    let jira = state.jira()?;
    let settings = state.settings();
    if settings.selection.project_keys.is_empty() {
        return Ok(vec![]);
    }
    let mut sprints = jira
        .search_sprints(&settings.selection.project_keys, &query, include_closed)
        .await
        .map_err(err)?;
    for s in &mut sprints {
        s.selected = settings.selection.sprint_ids.contains(&s.id);
    }
    Ok(sprints)
}

#[tauri::command]
pub async fn list_issue_types(state: S<'_>) -> Result<Vec<IssueType>, String> {
    let jira = state.jira()?;
    jira.issue_types().await.map_err(err)
}

#[tauri::command]
pub async fn set_project_selection(
    app: AppHandle,
    state: S<'_>,
    projects: Vec<Project>,
) -> Result<Settings, String> {
    let mut settings = state.settings();
    settings.selection.project_keys = projects.iter().map(|p| p.key.clone()).collect();
    settings.selection.tidy_project_filters();
    let palette = [
        "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#14b8a6", "#f97316",
    ];
    let mut rows = Vec::new();
    for (i, p) in projects.iter().enumerate() {
        let color = settings
            .project_colors
            .get(&p.key)
            .cloned()
            .or_else(|| p.color.clone())
            .unwrap_or_else(|| palette[i % palette.len()].to_string());
        settings.project_colors.insert(p.key.clone(), color.clone());
        rows.push(Project {
            color: Some(color),
            selected: true,
            ..p.clone()
        });
    }
    state.store.upsert_projects(&rows).map_err(err)?;
    // Sprints of removed projects are no longer meaningful.
    let known_sprints = state.store.list_sprints().map_err(err)?;
    let _ = known_sprints;
    save_settings(app, state, settings).await
}

#[tauri::command]
pub async fn set_sprint_selection(
    app: AppHandle,
    state: S<'_>,
    sprints: Vec<Sprint>,
) -> Result<Settings, String> {
    let mut settings = state.settings();
    settings.selection.sprint_ids = sprints.iter().map(|s| s.id).collect();
    let rows: Vec<Sprint> = sprints
        .into_iter()
        .map(|s| Sprint {
            selected: true,
            ..s
        })
        .collect();
    state.store.upsert_sprints(&rows).map_err(err)?;
    save_settings(app, state, settings).await
}

// -------------------------------------------------------------------- issues

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesSnapshot {
    pub issues: Vec<Issue>,
    pub projects: Vec<Project>,
    pub sprints: Vec<Sprint>,
    pub tracked_seconds: HashMap<String, i64>,
    pub refresh: RefreshStatus,
}

#[tauri::command]
pub fn get_issues(state: S<'_>) -> Result<IssuesSnapshot, String> {
    let settings = state.settings();
    let mut projects = state.store.list_projects().map_err(err)?;
    for p in &mut projects {
        if let Some(c) = settings.project_colors.get(&p.key) {
            p.color = Some(c.clone());
        }
    }
    Ok(IssuesSnapshot {
        issues: state.store.list_issues().map_err(err)?,
        projects,
        sprints: state.store.list_sprints().map_err(err)?,
        tracked_seconds: state
            .store
            .tracked_seconds_by_issue(Utc::now())
            .map_err(err)?,
        refresh: state.refresh.read().unwrap().clone(),
    })
}

#[tauri::command]
pub async fn refresh_issues(app: AppHandle, state: S<'_>) -> Result<RefreshStatus, String> {
    jobs::refresh_issues(&app, &state).await
}

#[tauri::command]
pub fn get_refresh_status(state: S<'_>) -> RefreshStatus {
    state.refresh.read().unwrap().clone()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteResult {
    /// `false` when the favorite limit is reached and nothing was changed.
    pub accepted: bool,
    pub limit: usize,
    pub count: usize,
}

#[tauri::command]
pub fn set_favorite(
    app: AppHandle,
    state: S<'_>,
    key: String,
    favorite: bool,
) -> Result<FavoriteResult, String> {
    let accepted = state.store.set_favorite(&key, favorite).map_err(err)?;
    let count = state.store.favorite_count().map_err(err)?;
    if accepted {
        emit(&app, "issues-updated", ());
        crate::tray::refresh(&app, &state);
    }
    Ok(FavoriteResult {
        accepted,
        limit: MAX_FAVORITES,
        count,
    })
}

/// Label suggestions for the per-project filter (site-wide JQL autocomplete).
#[tauri::command]
pub async fn search_labels(state: S<'_>, query: String) -> Result<Vec<String>, String> {
    let jira = state.jira()?;
    jira.search_labels(&query).await.map_err(|e| {
        tracing::warn!("label search for {query:?} failed: {e}");
        err(e)
    })
}

/// Component names of one project for the per-project filter.
#[tauri::command]
pub async fn list_components(state: S<'_>, project_key: String) -> Result<Vec<String>, String> {
    let jira = state.jira()?;
    jira.project_components(&project_key).await.map_err(|e| {
        tracing::warn!("component list for {project_key} failed: {e}");
        err(e)
    })
}

#[tauri::command]
pub async fn add_issue_by_key(app: AppHandle, state: S<'_>, key: String) -> Result<Issue, String> {
    let key = key.trim().to_uppercase();
    if let Some(existing) = state.store.get_issue(&key).map_err(err)? {
        return Ok(existing);
    }
    let jira = state.jira()?;
    let mut issue = jira.get_issue(&key).await.map_err(|e| match e {
        yattt_core::Error::Http { status: 404, .. } => format!("Ticket {key} wurde nicht gefunden"),
        other => other.to_string(),
    })?;
    issue.source = "manual".into();
    state
        .store
        .upsert_issues(std::slice::from_ref(&issue), Utc::now())
        .map_err(err)?;
    let known = state.store.list_projects().map_err(err)?;
    if !known.iter().any(|p| p.key == issue.project_key) {
        let _ = state.store.upsert_projects(&[Project {
            id: String::new(),
            key: issue.project_key.clone(),
            name: issue.project_key.clone(),
            avatar_url: None,
            color: None,
            selected: false,
        }]);
    }
    emit(&app, "issues-updated", ());
    Ok(issue)
}

#[tauri::command]
pub fn set_project_color(
    app: AppHandle,
    state: S<'_>,
    key: String,
    color: Option<String>,
) -> Result<(), String> {
    state
        .store
        .set_project_color(&key, color.as_deref())
        .map_err(err)?;
    {
        let mut s = state.settings.write().unwrap();
        match &color {
            Some(c) => {
                s.project_colors.insert(key.clone(), c.clone());
            }
            None => {
                s.project_colors.remove(&key);
            }
        }
        let _ = yattt_core::settings::save_settings(&state.paths, &s);
    }
    emit(&app, "issues-updated", ());
    Ok(())
}

#[tauri::command]
pub fn get_issue_url(state: S<'_>, key: String) -> String {
    format!("{}/browse/{}", state.settings().jira_base(), key)
}

// --------------------------------------------------------------------- timer

#[tauri::command]
pub fn start_timer(
    app: AppHandle,
    state: S<'_>,
    issue_key: String,
    note: Option<String>,
    at: Option<DateTime<Utc>>,
) -> Result<StartResult, String> {
    let settings = state.settings();
    let engine = TimerEngine::new(&state.store, &settings.timer);
    let result = engine
        .start(&issue_key, note.as_deref().unwrap_or(""), at, Utc::now())
        .map_err(err)?;
    state.idle.lock().unwrap().idle_since = None;
    crate::after_timer_change(&app, &state);
    if settings.timer.book_on_stop {
        let ids: Vec<i64> = result.stopped.iter().map(|e| e.id).collect();
        if !ids.is_empty() {
            spawn_book(&app, &state, ids);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn stop_timer(
    app: AppHandle,
    state: S<'_>,
    entry_id: Option<i64>,
    at: Option<DateTime<Utc>>,
    note: Option<String>,
) -> Result<Vec<StopResult>, String> {
    let settings = state.settings();
    let engine = TimerEngine::new(&state.store, &settings.timer);
    let results = match entry_id {
        Some(id) => vec![engine
            .stop(id, at, note.as_deref(), Utc::now())
            .map_err(err)?],
        None => engine.stop_all(at, Utc::now()).map_err(err)?,
    };
    crate::after_timer_change(&app, &state);
    if settings.timer.book_on_stop {
        let ids: Vec<i64> = results
            .iter()
            .filter_map(|r| r.entry.as_ref().map(|e| e.id))
            .collect();
        if !ids.is_empty() {
            spawn_book(&app, &state, ids);
        }
    }
    Ok(results)
}

fn spawn_book(app: &AppHandle, state: &Arc<AppState>, ids: Vec<i64>) {
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = jobs::book_entries(&app, &state, &ids).await {
            tracing::warn!("book on stop failed: {e}");
        }
    });
}

#[tauri::command]
pub fn get_running(state: S<'_>) -> Result<Vec<TimeEntry>, String> {
    state.store.running_entries().map_err(err)
}

#[tauri::command]
pub fn update_running(
    app: AppHandle,
    state: S<'_>,
    entry_id: i64,
    started_at: Option<DateTime<Utc>>,
    note: Option<String>,
) -> Result<TimeEntry, String> {
    let now = Utc::now();
    if let Some(n) = note {
        state.store.set_entry_note(entry_id, &n, now).map_err(err)?;
    }
    if let Some(s) = started_at {
        if s > now {
            return Err("Startzeit liegt in der Zukunft".into());
        }
        state.store.set_entry_start(entry_id, s, now).map_err(err)?;
    }
    let entry = state
        .store
        .get_entry(entry_id)
        .map_err(err)?
        .ok_or("Eintrag nicht gefunden")?;
    crate::after_timer_change(&app, &state);
    Ok(entry)
}

// ------------------------------------------------------------------- entries

#[tauri::command]
pub fn list_entries(
    state: S<'_>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<TimeEntry>, String> {
    state.store.list_entries(from, to).map_err(err)
}

#[tauri::command]
pub fn add_entry(app: AppHandle, state: S<'_>, input: TimeEntryInput) -> Result<TimeEntry, String> {
    if input.ended_at.is_none() {
        return Err("Manuelle Einträge brauchen ein Ende".into());
    }
    let entry = state.store.insert_entry(&input, Utc::now()).map_err(err)?;
    emit(&app, "entries-changed", ());
    crate::tray::refresh(&app, &state);
    Ok(entry)
}

#[tauri::command]
pub fn update_entry(
    app: AppHandle,
    state: S<'_>,
    id: i64,
    input: TimeEntryInput,
) -> Result<TimeEntry, String> {
    let entry = state
        .store
        .update_entry(id, &input, Utc::now())
        .map_err(err)?;
    crate::after_timer_change(&app, &state);
    Ok(entry)
}

#[tauri::command]
pub fn delete_entry(app: AppHandle, state: S<'_>, id: i64) -> Result<(), String> {
    let entry = state.store.soft_delete_entry(id, Utc::now()).map_err(err)?;
    if let Some(e) = entry {
        if e.jira_worklog_id.is_none() && e.tempo_worklog_id.is_none() {
            // Nothing remote to clean up: keep it soft-deleted for undo, purge later.
        }
    }
    crate::after_timer_change(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn restore_entry(app: AppHandle, state: S<'_>, id: i64) -> Result<Option<TimeEntry>, String> {
    let entry = state.store.restore_entry(id, Utc::now()).map_err(err)?;
    crate::after_timer_change(&app, &state);
    Ok(entry)
}

// ---------------------------------------------------------------------- sync

#[tauri::command]
pub async fn book_entries(
    app: AppHandle,
    state: S<'_>,
    ids: Vec<i64>,
) -> Result<SyncSummary, String> {
    jobs::book_entries(&app, &state, &ids).await
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, state: S<'_>) -> Result<SyncSummary, String> {
    jobs::sync_pending(&app, &state).await
}

#[tauri::command]
pub async fn import_worklogs(
    app: AppHandle,
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<usize, String> {
    let jira = state.jira()?;
    let user = state.user().ok_or("Nicht angemeldet")?;
    let tempo_cloud = state.tempo_cloud.read().unwrap().clone();
    let ctx = jobs::booking_context(&state, &jira, &user, tempo_cloud.as_deref());
    let n = yattt_core::sync::import_worklogs(&ctx, &state.store, from, to, Utc::now())
        .await
        .map_err(err)?;
    emit(&app, "entries-changed", ());
    emit(&app, "issues-updated", ());
    Ok(n)
}

// --------------------------------------------------------------- stats/export

fn local_day_start(date: NaiveDate) -> DateTime<Utc> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap()))
}

fn range_entries(
    state: &AppState,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<TimeEntry>, String> {
    let start = local_day_start(from);
    let end = local_day_start(to + Duration::days(1));
    state.store.list_entries(start, end).map_err(err)
}

#[tauri::command]
pub fn get_stats(state: S<'_>, from: NaiveDate, to: NaiveDate) -> Result<Stats, String> {
    let entries = range_entries(&state, from, to)?;
    let issues: HashMap<String, Issue> = state
        .store
        .list_issues()
        .map_err(err)?
        .into_iter()
        .map(|i| (i.key.clone(), i))
        .collect();
    let settings = state.settings();
    let projects: HashMap<String, Project> = state
        .store
        .list_projects()
        .map_err(err)?
        .into_iter()
        .map(|mut p| {
            if let Some(c) = settings.project_colors.get(&p.key) {
                p.color = Some(c.clone());
            }
            (p.key.clone(), p)
        })
        .collect();
    Ok(yattt_core::stats::compute(
        &entries,
        &issues,
        &projects,
        from,
        to,
        &settings.work,
        Utc::now(),
    ))
}

#[tauri::command]
pub fn export_entries(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    format: String,
    summarize: bool,
    path: String,
) -> Result<usize, String> {
    let entries = range_entries(&state, from, to)?;
    let issues: HashMap<String, Issue> = state
        .store
        .list_issues()
        .map_err(err)?
        .into_iter()
        .map(|i| (i.key.clone(), i))
        .collect();
    let mut rows = yattt_core::export::build_rows(&entries, &issues, Utc::now());
    if summarize {
        rows = yattt_core::export::summarize_rows(&rows);
    }
    let settings = state.settings();
    let content = match format.as_str() {
        "csv" => yattt_core::export::to_csv(&rows, settings.work.time_format).map_err(err)?,
        "json" => yattt_core::export::to_json(&rows).map_err(err)?,
        other => return Err(format!("Unbekanntes Format {other}")),
    };
    std::fs::write(&path, content).map_err(err)?;
    Ok(rows.len())
}

#[tauri::command]
pub fn get_export_rows(
    state: S<'_>,
    from: NaiveDate,
    to: NaiveDate,
    summarize: bool,
) -> Result<Vec<yattt_core::export::ExportRow>, String> {
    let entries = range_entries(&state, from, to)?;
    let issues: HashMap<String, Issue> = state
        .store
        .list_issues()
        .map_err(err)?
        .into_iter()
        .map(|i| (i.key.clone(), i))
        .collect();
    let rows = yattt_core::export::build_rows(&entries, &issues, Utc::now());
    Ok(if summarize {
        yattt_core::export::summarize_rows(&rows)
    } else {
        rows
    })
}

// --------------------------------------------------------------- idle/restore

#[tauri::command]
pub fn get_idle_prompt(state: S<'_>) -> Option<IdlePrompt> {
    state.idle.lock().unwrap().prompt.clone()
}

#[tauri::command]
pub fn respond_idle(app: AppHandle, state: S<'_>, decision: IdleDecision) -> Result<(), String> {
    let prompt = state.idle.lock().unwrap().prompt.take();
    let Some(prompt) = prompt else { return Ok(()) };
    let settings = state.settings();
    let engine = TimerEngine::new(&state.store, &settings.timer);
    for id in prompt.entry_ids {
        engine
            .apply_idle_decision(id, prompt.idle_since, decision, Utc::now())
            .map_err(err)?;
    }
    state.idle.lock().unwrap().idle_since = None;
    crate::after_timer_change(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn get_restore_prompt(state: S<'_>) -> Option<RestorePrompt> {
    state.restore.lock().unwrap().clone()
}

#[tauri::command]
pub fn respond_restore(
    app: AppHandle,
    state: S<'_>,
    entry_id: i64,
    decision: RestoreDecision,
) -> Result<(), String> {
    let last_seen = state
        .restore
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|p| p.last_seen);
    let settings = state.settings();
    let engine = TimerEngine::new(&state.store, &settings.timer);
    engine
        .apply_restore_decision(entry_id, last_seen, decision, Utc::now())
        .map_err(err)?;
    {
        let mut restore = state.restore.lock().unwrap();
        if let Some(p) = restore.as_mut() {
            p.entries.retain(|e| e.id != entry_id);
            if p.entries.is_empty() {
                *restore = None;
            }
        }
    }
    crate::after_timer_change(&app, &state);
    Ok(())
}

// ------------------------------------------------------------- exit/shutdown

#[tauri::command]
pub fn request_quit(app: AppHandle, state: S<'_>) {
    crate::request_quit(&app, &state);
}

#[tauri::command]
pub fn confirm_exit(app: AppHandle, state: S<'_>, stop_timers: bool) -> Result<(), String> {
    crate::shutdown::apply_choice(
        &state,
        if stop_timers {
            crate::shutdown::ShutdownChoice::StopAndQuit
        } else {
            crate::shutdown::ShutdownChoice::KeepAndQuit
        },
    );
    crate::shutdown::clear_block_reason(&app);
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn cancel_exit(app: AppHandle, state: S<'_>) {
    state
        .shutdown_prompt_open
        .store(false, std::sync::atomic::Ordering::SeqCst);
    crate::shutdown::clear_block_reason(&app);
}

// ---------------------------------------------------------------------- tempo

#[tauri::command]
pub async fn set_tempo_cloud_token(
    app: AppHandle,
    state: S<'_>,
    token: Option<String>,
) -> Result<(), String> {
    let settings = state.settings();
    let key = secrets::tempo_token_key(&settings.jira_base());
    match token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        Some(t) => {
            let client = yattt_core::tempo::TempoCloud::new(&settings.tempo.cloud_api_base, &t)
                .map_err(err)?;
            client
                .verify()
                .await
                .map_err(|e| format!("Tempo-Token wurde nicht akzeptiert: {e}"))?;
            state.secrets.set(&key, &t).map_err(err)?;
        }
        None => state.secrets.delete(&key).map_err(err)?,
    }
    jobs::load_tempo_cloud(&state);
    {
        let mut s = state.settings.write().unwrap();
        s.tempo.cloud_token_set = state.tempo_cloud.read().unwrap().is_some();
        let _ = yattt_core::settings::save_settings(&state.paths, &s);
    }
    emit(&app, "settings-changed", state.settings());
    Ok(())
}

// ----------------------------------------------------------------------- info

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub config_dir: String,
    pub data_dir: String,
    pub log_dir: String,
    pub secret_backend: yattt_core::secrets::SecretBackend,
}

#[tauri::command]
pub fn get_app_info(state: S<'_>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        config_dir: state.paths.config_dir.display().to_string(),
        data_dir: state.paths.data_dir.display().to_string(),
        log_dir: state.paths.data_dir.join("logs").display().to_string(),
        secret_backend: state.secrets.backend(),
    }
}
