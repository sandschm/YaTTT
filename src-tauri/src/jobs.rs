//! Long-running operations shared by commands, scheduler and tray: session restore,
//! issue refresh, booking.

use crate::state::{AppState, RefreshStatus};
use chrono::Utc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use yattt_core::jira::jql::build_issue_jql;
use yattt_core::jira::JiraClient;
use yattt_core::models::*;
use yattt_core::secrets;
use yattt_core::settings::TempoMode;
use yattt_core::sync::{BookingContext, SyncSummary};
use yattt_core::tempo::TempoCloud;
use yattt_core::Error;

pub fn emit<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("emit {event} failed: {e}");
    }
}

/// Build a client from a credential, verify it and install it into the state.
pub async fn install_client(
    app: &AppHandle,
    state: &Arc<AppState>,
    credential: Credential,
) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err(
            yattt_core::tr!("Keine Jira-URL konfiguriert", "No Jira URL configured").into(),
        );
    }
    let deployment = state
        .server
        .read()
        .unwrap()
        .as_ref()
        .map(|s| s.deployment)
        .unwrap_or_default();
    let client =
        Arc::new(JiraClient::new(&base, credential, deployment).map_err(|e| e.to_string())?);
    let (user, server) = match client.verify().await {
        Ok(v) => v,
        Err(Error::AuthExpired(code)) => {
            return Err(yattt_core::tr!(
                "Anmeldung nicht akzeptiert (HTTP {code})",
                "Sign-in not accepted (HTTP {code})",
                code = code
            ));
        }
        Err(e) => return Err(e.to_string()),
    };
    // Persist the (possibly rotated) credential and keep it updated.
    if let Some(cookies) = client.cookies() {
        let _ = secrets::save_credential(
            state.secrets.as_ref(),
            &base,
            &Credential::Cookies { cookies },
        );
    }
    {
        let secrets = state.secrets.clone();
        let base_for_listener = base.clone();
        client.set_cookie_listener(Arc::new(move |cookies| {
            let cred = Credential::Cookies {
                cookies: cookies.to_vec(),
            };
            if let Err(e) = secrets::save_credential(secrets.as_ref(), &base_for_listener, &cred) {
                tracing::warn!("could not persist rotated cookies: {e}");
            }
        }));
    }
    state.set_session(client, user, server);
    load_tempo_cloud(state);
    let status = state.auth_status();
    emit(app, "auth-changed", status.clone());
    Ok(status)
}

pub fn load_tempo_cloud(state: &Arc<AppState>) {
    let settings = state.settings();
    let token = state
        .secrets
        .get(&secrets::tempo_token_key(&settings.jira_base()))
        .ok()
        .flatten();
    let client = token
        .and_then(|t| TempoCloud::new(&settings.tempo.cloud_api_base, &t).ok())
        .map(Arc::new);
    *state.tempo_cloud.write().unwrap() = client;
}

/// Try to restore a session from the secret store at startup.
pub async fn restore_session(app: &AppHandle, state: &Arc<AppState>) {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return;
    }
    match secrets::load_credential(state.secrets.as_ref(), &base) {
        Ok(Some(credential)) => {
            if let Err(e) = install_client(app, state, credential).await {
                tracing::info!("stored session not usable: {e}");
                state.clear_session(Some(
                    yattt_core::tr!(
                        "Gespeicherte Sitzung ist nicht mehr gültig. Bitte erneut anmelden.",
                        "The stored session is no longer valid. Please sign in again."
                    )
                    .into(),
                ));
                emit(app, "auth-changed", state.auth_status());
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("secret store error: {e}");
            state.clear_session(Some(yattt_core::tr!(
                "Zugangsdaten konnten nicht gelesen werden: {e}",
                "Credentials could not be read: {e}",
                e = e
            )));
            emit(app, "auth-changed", state.auth_status());
        }
    }
}

fn handle_auth_error(app: &AppHandle, state: &Arc<AppState>, e: &Error) {
    if let Error::AuthExpired(_) = e {
        state.mark_auth_expired();
        emit(app, "auth-changed", state.auth_status());
    }
}

/// Reload the issue list from Jira according to the selection settings.
pub async fn refresh_issues(
    app: &AppHandle,
    state: &Arc<AppState>,
) -> Result<RefreshStatus, String> {
    if state.refresh_in_progress.swap(true, Ordering::SeqCst) {
        return Ok(state.refresh.read().unwrap().clone());
    }
    let result = refresh_inner(app, state).await;
    state.refresh_in_progress.store(false, Ordering::SeqCst);
    let mut status = state.refresh.write().unwrap();
    status.in_progress = false;
    match &result {
        Ok(count) => {
            status.last_success = Some(Utc::now());
            status.last_error = None;
            status.issue_count = *count;
        }
        Err(e) => status.last_error = Some(e.clone()),
    }
    let snapshot = status.clone();
    drop(status);
    emit(app, "refresh-status", snapshot.clone());
    if result.is_ok() {
        emit(app, "issues-updated", ());
        crate::tray::refresh(app, state);
    }
    result.map(|_| snapshot)
}

async fn refresh_inner(app: &AppHandle, state: &Arc<AppState>) -> Result<usize, String> {
    let jira = state.jira()?;
    let settings = state.settings();
    {
        let mut status = state.refresh.write().unwrap();
        status.in_progress = true;
        emit(app, "refresh-status", status.clone());
    }
    let now = Utc::now();
    let Some(jql) = build_issue_jql(&settings.selection) else {
        // Nothing selected: everything that only came from sprints disappears.
        state
            .store
            .prune_stale_issues(now)
            .map_err(|e| e.to_string())?;
        state
            .store
            .demote_unseen_issues(now)
            .map_err(|e| e.to_string())?;
        return Ok(0);
    };
    tracing::debug!("refresh jql: {jql}");
    let issues = match jira.search_issues(&jql, 1000).await {
        Ok(v) => v,
        Err(e) => {
            handle_auth_error(app, state, &e);
            let extra = settings.selection.extra_jql.trim();
            let message = match &e {
                yattt_core::Error::Http { status: 400, .. } if !extra.is_empty() => {
                    yattt_core::tr!(
                        "{e} – vermutlich wegen der zusätzlichen JQL-Bedingung „{extra}“ \
                     (Einstellungen → Projekte & Sprints). Labels und Komponenten je Projekt \
                     werden dort direkt zugeordnet.",
                        "{e} – probably because of the additional JQL condition \"{extra}\" \
                     (Settings → Projects & Sprints). Labels and components per project are \
                     assigned there directly.",
                        e = e,
                        extra = extra
                    )
                }
                _ => e.to_string(),
            };
            return Err(message);
        }
    };
    let count = issues.len();
    state
        .store
        .upsert_issues(&issues, now)
        .map_err(|e| e.to_string())?;
    state
        .store
        .prune_stale_issues(now)
        .map_err(|e| e.to_string())?;
    state
        .store
        .demote_unseen_issues(now)
        .map_err(|e| e.to_string())?;
    // Keep project rows for all projects we saw.
    let known: std::collections::HashSet<String> = state
        .store
        .list_projects()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| p.key)
        .collect();
    let mut missing: Vec<Project> = Vec::new();
    for key in issues
        .iter()
        .map(|i| i.project_key.clone())
        .collect::<std::collections::HashSet<_>>()
    {
        if !known.contains(&key) {
            missing.push(Project {
                id: String::new(),
                key: key.clone(),
                name: key.clone(),
                avatar_url: None,
                color: None,
                selected: settings.selection.project_keys.contains(&key),
            });
        }
    }
    if !missing.is_empty() {
        state
            .store
            .upsert_projects(&missing)
            .map_err(|e| e.to_string())?;
    }
    Ok(count)
}

pub fn booking_context<'a>(
    state: &'a AppState,
    jira: &'a JiraClient,
    user: &'a JiraUser,
    tempo_cloud: Option<&'a TempoCloud>,
) -> BookingContext<'a> {
    let settings = state.settings();
    let mode = match settings.tempo.mode {
        TempoMode::TempoCloud if tempo_cloud.is_none() => TempoMode::JiraWorklog,
        m => m,
    };
    BookingContext {
        jira,
        tempo_cloud,
        mode,
        user,
        rounding: settings.timer.rounding,
    }
}

/// Book the given entries now (they are marked pending first).
pub async fn book_entries(
    app: &AppHandle,
    state: &Arc<AppState>,
    ids: &[i64],
) -> Result<SyncSummary, String> {
    let now = Utc::now();
    for id in ids {
        if let Some(entry) = state.store.get_entry(*id).map_err(|e| e.to_string())? {
            if entry.is_running() {
                return Err(yattt_core::tr!(
                    "Eintrag {id} läuft noch",
                    "Entry {id} is still running",
                    id = id
                ));
            }
            if entry.sync_state != SyncState::Synced {
                state
                    .store
                    .set_sync_state(*id, SyncState::Pending, None, now)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    sync_pending(app, state).await
}

/// Process everything pending/modified/deleted.
pub async fn sync_pending(app: &AppHandle, state: &Arc<AppState>) -> Result<SyncSummary, String> {
    if state.sync_in_progress.swap(true, Ordering::SeqCst) {
        return Ok(SyncSummary::default());
    }
    let result = async {
        let jira = state.jira()?;
        let user = state
            .user()
            .ok_or_else(|| yattt_core::tr!("Nicht angemeldet", "Not signed in").to_string())?;
        let tempo_cloud = state.tempo_cloud.read().unwrap().clone();
        let ctx = booking_context(state, &jira, &user, tempo_cloud.as_deref());
        match yattt_core::sync::process_pending(&ctx, &state.store, Utc::now()).await {
            Ok(summary) => Ok(summary),
            Err(e) => {
                handle_auth_error(app, state, &e);
                Err(e.to_string())
            }
        }
    }
    .await;
    state.sync_in_progress.store(false, Ordering::SeqCst);
    emit(app, "entries-changed", ());
    if let Ok(summary) = &result {
        emit(app, "sync-done", summary.clone());
    }
    result
}

pub fn has_pending_work(state: &AppState) -> bool {
    let pending = state
        .store
        .entries_with_state(SyncState::Pending)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let modified = state
        .store
        .entries_with_state(SyncState::Modified)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let deleted = state
        .store
        .deleted_entries_with_remote()
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    pending || modified || deleted
}
