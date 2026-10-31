//! Shared data model. Everything here is serialized to the frontend, so field names are camelCase.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How the Jira instance is deployed. Affects endpoints and pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Deployment {
    #[default]
    Unknown,
    Cloud,
    DataCenter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub base_url: String,
    pub version: String,
    pub deployment: Deployment,
    pub server_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    /// Cloud: accountId, Data Center: username (`name`).
    pub id: String,
    /// Data Center user key (e.g. `JIRAUSER10100`), needed by Tempo DC as `worker`.
    pub key: Option<String>,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    /// IANA time zone of the Jira profile.
    pub time_zone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub key: String,
    pub name: String,
    pub avatar_url: Option<String>,
    /// Hex color chosen locally (e.g. "#3b82f6").
    pub color: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub board_type: String,
    pub project_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub id: i64,
    pub name: String,
    /// "active" | "future" | "closed"
    pub state: String,
    pub board_id: Option<i64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub goal: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IssueType {
    pub id: String,
    pub name: String,
    pub subtask: bool,
    pub icon_url: Option<String>,
}

/// Upper bound for favorites: they are listed in the tray menu, which must stay short.
pub const MAX_FAVORITES: usize = 15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub project_key: String,
    pub summary: String,
    pub issue_type: String,
    pub issue_type_id: Option<String>,
    pub issue_type_icon: Option<String>,
    pub is_subtask: bool,
    pub status: String,
    /// "new" | "indeterminate" | "done" | "undefined"
    pub status_category: String,
    pub parent_key: Option<String>,
    pub epic_key: Option<String>,
    pub sprint_id: Option<i64>,
    pub sprint_name: Option<String>,
    pub assignee: Option<String>,
    pub assignee_id: Option<String>,
    pub priority: Option<String>,
    pub due_date: Option<String>,
    pub original_estimate_s: Option<i64>,
    pub remaining_estimate_s: Option<i64>,
    pub time_spent_s: Option<i64>,
    pub updated: Option<String>,
    /// Jira labels of the issue (used for the per-project label filter and the ticket list filter).
    #[serde(default)]
    pub labels: Vec<String>,
    /// Component names of the issue.
    #[serde(default)]
    pub components: Vec<String>,
    pub favorite: bool,
    /// Where this issue came from: "sprint" (open sprint of a selected project), "selected-sprint", "manual" (started by key / search), "tracked".
    pub source: String,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    /// Only stored locally, never booked.
    #[default]
    Local,
    /// Marked for booking, waiting for the sync worker.
    Pending,
    /// Booked to Jira/Tempo; ids are set.
    Synced,
    /// Booked, but changed locally afterwards. Needs update.
    Modified,
    /// Booking failed, see `sync_error`.
    Failed,
}

impl SyncState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncState::Local => "local",
            SyncState::Pending => "pending",
            SyncState::Synced => "synced",
            SyncState::Modified => "modified",
            SyncState::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> SyncState {
        match s {
            "pending" => SyncState::Pending,
            "synced" => SyncState::Synced,
            "modified" => SyncState::Modified,
            "failed" => SyncState::Failed,
            _ => SyncState::Local,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub id: i64,
    pub issue_key: String,
    pub started_at: DateTime<Utc>,
    /// `None` while the timer is running.
    pub ended_at: Option<DateTime<Utc>>,
    pub note: String,
    pub billable: bool,
    pub sync_state: SyncState,
    pub jira_worklog_id: Option<String>,
    pub tempo_worklog_id: Option<String>,
    pub sync_error: Option<String>,
    /// Seconds actually booked to Jira (after rounding). `None` if never booked.
    pub booked_seconds: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TimeEntry {
    pub fn is_running(&self) -> bool {
        self.ended_at.is_none()
    }

    /// Duration in seconds, using `now` for a running entry.
    pub fn duration_seconds(&self, now: DateTime<Utc>) -> i64 {
        let end = self.ended_at.unwrap_or(now);
        (end - self.started_at).num_seconds().max(0)
    }
}

/// Input for creating or updating an entry from the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryInput {
    pub issue_key: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_true")]
    pub billable: bool,
}

fn default_true() -> bool {
    true
}

/// A browser cookie captured from the login webview and reused for REST calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
}

/// Credentials stored in the OS keyring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Credential {
    /// Session cookies captured from a browser login (SSO, 2FA).
    Cookies { cookies: Vec<StoredCookie> },
    /// Username + password (Data Center). Sent as Basic auth.
    Basic { username: String, password: String },
    /// Personal Access Token (Data Center) – sent as Bearer.
    Bearer { token: String },
    /// E-mail + API token (Cloud) – sent as Basic auth.
    ApiToken { email: String, token: String },
}

impl Credential {
    pub fn kind(&self) -> &'static str {
        match self {
            Credential::Cookies { .. } => "cookies",
            Credential::Basic { .. } => "basic",
            Credential::Bearer { .. } => "bearer",
            Credential::ApiToken { .. } => "apiToken",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    pub authenticated: bool,
    pub method: Option<String>,
    pub user: Option<JiraUser>,
    pub server: Option<ServerInfo>,
    pub message: Option<String>,
}

/// Result of a booking to Jira/Tempo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookingResult {
    pub entry_id: i64,
    pub jira_worklog_id: Option<String>,
    pub tempo_worklog_id: Option<String>,
    pub booked_seconds: i64,
}

/// What the user decided after an idle period / sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdleDecision {
    /// Keep the time, the user was working away from the keyboard.
    Keep,
    /// Stop the timer at the moment the idle period began.
    StopAtIdleStart,
    /// Remove the idle period from the entry but keep the timer running from now.
    DiscardIdle,
}

/// What the user decided for a timer that was still running when the app last ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestoreDecision {
    /// Timer keeps running (nothing happened in between).
    KeepRunning,
    /// Stop at the last heartbeat / last known time.
    StopAtLastSeen,
    /// Stop now.
    StopNow,
    /// Delete the entry.
    Discard,
}
