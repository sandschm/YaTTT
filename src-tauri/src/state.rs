//! Shared application state behind `tauri::State<Arc<AppState>>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use yattt_core::jira::JiraClient;
use yattt_core::models::*;
use yattt_core::secrets::{EncryptedSecretStore, SecretStore};
use yattt_core::settings::{AppPaths, Settings};
use yattt_core::storage::Store;
use yattt_core::tempo::TempoCloud;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefreshStatus {
    pub in_progress: bool,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub issue_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdlePrompt {
    pub entry_ids: Vec<i64>,
    pub idle_since: DateTime<Utc>,
    /// "idle" or "sleep"
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestorePrompt {
    pub entries: Vec<TimeEntry>,
    pub last_seen: Option<DateTime<Utc>>,
    /// Set when the previous session ended through a shutdown/quit where the user chose to keep running.
    pub kept_running_on_exit: bool,
}

#[derive(Default)]
pub struct IdleTracker {
    pub idle_since: Option<DateTime<Utc>>,
    pub prompt: Option<IdlePrompt>,
    pub last_tick: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct ReminderState {
    pub last_no_timer_reminder: Option<chrono::NaiveDate>,
    pub last_goal_reminder: Option<chrono::NaiveDate>,
    pub last_break_reminder: Option<DateTime<Utc>>,
    pub last_long_running_warning: Option<DateTime<Utc>>,
    pub last_unbooked_reminder: Option<chrono::NaiveDate>,
}

pub struct AppState {
    pub paths: AppPaths,
    pub store: Arc<Store>,
    pub settings: RwLock<Settings>,
    pub secrets: Arc<dyn SecretStore>,
    pub jira: RwLock<Option<Arc<JiraClient>>>,
    pub tempo_cloud: RwLock<Option<Arc<TempoCloud>>>,
    pub user: RwLock<Option<JiraUser>>,
    pub server: RwLock<Option<ServerInfo>>,
    pub auth_message: RwLock<Option<String>>,
    pub refresh: RwLock<RefreshStatus>,
    pub idle: Mutex<IdleTracker>,
    pub reminders: Mutex<ReminderState>,
    pub restore: Mutex<Option<RestorePrompt>>,
    pub exit_confirmed: AtomicBool,
    pub login_in_progress: AtomicBool,
    pub refresh_in_progress: AtomicBool,
    pub sync_in_progress: AtomicBool,
    pub shutdown_prompt_open: AtomicBool,
}

impl AppState {
    pub fn init(paths: AppPaths) -> yattt_core::Result<Self> {
        paths.ensure()?;
        let settings = yattt_core::settings::load_settings(&paths).unwrap_or_else(|e| {
            tracing::warn!("settings could not be loaded, using defaults: {e}");
            Settings::default()
        });
        let store = Arc::new(Store::open(&paths.database_file())?);
        let secrets: Arc<dyn SecretStore> = Arc::new(EncryptedSecretStore::new(&paths));
        Ok(Self {
            paths,
            store,
            settings: RwLock::new(settings),
            secrets,
            jira: RwLock::new(None),
            tempo_cloud: RwLock::new(None),
            user: RwLock::new(None),
            server: RwLock::new(None),
            auth_message: RwLock::new(None),
            refresh: RwLock::new(RefreshStatus::default()),
            idle: Mutex::new(IdleTracker::default()),
            reminders: Mutex::new(ReminderState::default()),
            restore: Mutex::new(None),
            exit_confirmed: AtomicBool::new(false),
            login_in_progress: AtomicBool::new(false),
            refresh_in_progress: AtomicBool::new(false),
            sync_in_progress: AtomicBool::new(false),
            shutdown_prompt_open: AtomicBool::new(false),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn jira(&self) -> Result<Arc<JiraClient>, String> {
        self.jira
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "Nicht bei Jira angemeldet".to_string())
    }

    pub fn jira_opt(&self) -> Option<Arc<JiraClient>> {
        self.jira.read().unwrap().clone()
    }

    pub fn user(&self) -> Option<JiraUser> {
        self.user.read().unwrap().clone()
    }

    pub fn is_authenticated(&self) -> bool {
        self.jira.read().unwrap().is_some() && self.user.read().unwrap().is_some()
    }

    pub fn auth_status(&self) -> AuthStatus {
        let settings = self.settings();
        AuthStatus {
            configured: !settings.jira.base_url.is_empty(),
            authenticated: self.is_authenticated(),
            method: self.jira_opt().map(|j| j.credential_kind().to_string()),
            user: self.user(),
            server: self.server.read().unwrap().clone(),
            message: self.auth_message.read().unwrap().clone(),
        }
    }

    /// Install a verified client.
    pub fn set_session(&self, client: Arc<JiraClient>, user: JiraUser, server: ServerInfo) {
        *self.jira.write().unwrap() = Some(client);
        *self.user.write().unwrap() = Some(user);
        *self.server.write().unwrap() = Some(server);
        *self.auth_message.write().unwrap() = None;
    }

    pub fn clear_session(&self, message: Option<String>) {
        *self.jira.write().unwrap() = None;
        *self.user.write().unwrap() = None;
        *self.auth_message.write().unwrap() = message;
    }

    pub fn mark_auth_expired(&self) {
        self.clear_session(Some(
            "Die Jira-Sitzung ist abgelaufen. Bitte erneut anmelden.".into(),
        ));
    }

    pub fn exit_confirmed(&self) -> bool {
        self.exit_confirmed.load(Ordering::SeqCst)
    }

    pub fn running_count(&self) -> usize {
        self.store.running_entries().map(|v| v.len()).unwrap_or(0)
    }

    pub fn write_heartbeat(&self, now: DateTime<Utc>) {
        let _ = self.store.set_meta("last_heartbeat", &now.to_rfc3339());
    }

    pub fn last_heartbeat(&self) -> Option<DateTime<Utc>> {
        self.store
            .get_meta("last_heartbeat")
            .ok()
            .flatten()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc))
    }
}
