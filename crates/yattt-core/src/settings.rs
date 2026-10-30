//! Application settings. Stored as JSON in the user's config directory.
//! Secrets never live here, they go to [`crate::secrets`].

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "YaTTT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    /// Login in an embedded browser window; session cookies are reused (SSO, 2FA).
    #[default]
    BrowserSession,
    /// Username and password (Jira Data Center).
    Password,
    /// Personal Access Token (Jira Data Center).
    PersonalAccessToken,
    /// E-mail and API token (Jira Cloud).
    ApiToken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JiraSettings {
    pub base_url: String,
    pub auth_method: AuthMethod,
    /// Display only, the actual secret is in the keyring.
    pub username: Option<String>,
}

impl Default for JiraSettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            auth_method: AuthMethod::BrowserSession,
            username: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SelectionSettings {
    pub project_keys: Vec<String>,
    pub sprint_ids: Vec<i64>,
    /// Issue type names that are hidden (e.g. "Epic").
    pub excluded_issue_types: Vec<String>,
    /// Always load the issues of the currently open sprints of all selected projects.
    pub include_open_sprints: bool,
    pub only_assigned_to_me: bool,
    pub hide_done: bool,
    /// Optional additional JQL that is AND-ed to the generated query.
    pub extra_jql: String,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            project_keys: vec![],
            sprint_ids: vec![],
            excluded_issue_types: vec!["Epic".to_string()],
            include_open_sprints: true,
            only_assigned_to_me: false,
            hide_done: true,
            extra_jql: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RefreshSettings {
    pub auto: bool,
    pub interval_minutes: u32,
}

impl Default for RefreshSettings {
    fn default() -> Self {
        Self {
            auto: true,
            interval_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RoundingMode {
    #[default]
    Nearest,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rounding {
    /// 0 = no rounding.
    pub minutes: u32,
    pub mode: RoundingMode,
}

impl Default for Rounding {
    fn default() -> Self {
        Self {
            minutes: 0,
            mode: RoundingMode::Nearest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TimerSettings {
    pub allow_concurrent: bool,
    /// 0 disables idle detection.
    pub idle_timeout_minutes: u32,
    /// Entries shorter than this are discarded when stopped. 0 keeps everything.
    pub ignore_short_entries_seconds: u32,
    pub ask_note_on_stop: bool,
    /// Book the entry to Jira right when the timer stops.
    pub book_on_stop: bool,
    pub rounding: Rounding,
    pub default_billable: bool,
    /// Warn when a timer runs longer than this many hours (0 = off).
    pub long_running_warning_hours: u32,
}

impl Default for TimerSettings {
    fn default() -> Self {
        Self {
            allow_concurrent: false,
            idle_timeout_minutes: 10,
            ignore_short_entries_seconds: 60,
            ask_note_on_stop: false,
            book_on_stop: false,
            rounding: Rounding::default(),
            default_billable: true,
            long_running_warning_hours: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimeFormat {
    #[default]
    HoursMinutes,
    Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkSettings {
    /// Target hours Monday..Sunday.
    pub hours_per_weekday: [f32; 7],
    /// 0 = Monday … 6 = Sunday
    pub first_day_of_week: u8,
    pub time_format: TimeFormat,
    /// Remind to take a break after this many minutes of continuous tracking (0 = off).
    pub break_reminder_after_minutes: u32,
    /// "HH:MM"; remind when no timer runs after this time on a workday (empty = off).
    pub workday_start: String,
    pub daily_goal_notification: bool,
    pub unbooked_reminder: bool,
}

impl Default for WorkSettings {
    fn default() -> Self {
        Self {
            hours_per_weekday: [8.0, 8.0, 8.0, 8.0, 8.0, 0.0, 0.0],
            first_day_of_week: 0,
            time_format: TimeFormat::HoursMinutes,
            break_reminder_after_minutes: 240,
            workday_start: "09:30".to_string(),
            daily_goal_notification: true,
            unbooked_reminder: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BehaviorSettings {
    pub close_to_tray: bool,
    pub autostart: bool,
    pub notifications: bool,
    pub theme: Theme,
    pub shortcut_toggle_timer: String,
    pub shortcut_show_window: String,
    pub show_timer_in_tray: bool,
}

impl Default for BehaviorSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            autostart: false,
            notifications: true,
            theme: Theme::System,
            shortcut_toggle_timer: "CommandOrControl+Shift+T".to_string(),
            shortcut_show_window: "CommandOrControl+Shift+Y".to_string(),
            show_timer_in_tray: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TempoMode {
    /// Create Jira worklogs; Tempo picks them up automatically.
    #[default]
    JiraWorklog,
    /// POST /rest/tempo-timesheets/4/worklogs with the Jira session (Tempo Data Center).
    TempoDataCenter,
    /// POST https://api.tempo.io/4/worklogs with a Tempo API token (Tempo Cloud).
    TempoCloud,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TempoSettings {
    pub mode: TempoMode,
    /// Set when a Tempo Cloud token is present in the keyring.
    pub cloud_token_set: bool,
    pub cloud_api_base: String,
}

impl Default for TempoSettings {
    fn default() -> Self {
        Self {
            mode: TempoMode::JiraWorklog,
            cloud_token_set: false,
            cloud_api_base: "https://api.tempo.io/4".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub jira: JiraSettings,
    pub selection: SelectionSettings,
    pub refresh: RefreshSettings,
    pub timer: TimerSettings,
    pub work: WorkSettings,
    pub behavior: BehaviorSettings,
    pub tempo: TempoSettings,
    /// Project key -> hex color.
    pub project_colors: BTreeMap<String, String>,
    pub onboarding_done: bool,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !self.jira.base_url.is_empty() {
            let url = url::Url::parse(&self.jira.base_url)?;
            if url.scheme() != "https" && url.scheme() != "http" {
                return Err(Error::InvalidUrl("Nur http(s) wird unterstützt".into()));
            }
        }
        if self.refresh.interval_minutes == 0 && self.refresh.auto {
            return Err(Error::Invalid(
                "Aktualisierungsintervall muss mindestens 1 Minute sein".into(),
            ));
        }
        Ok(())
    }

    /// Base URL without trailing slash.
    pub fn jira_base(&self) -> String {
        self.jira.base_url.trim_end_matches('/').to_string()
    }
}

/// Well-known locations for app data.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| Error::Io("Kein Konfigurationsverzeichnis gefunden".into()))?
            .join(APP_DIR_NAME);
        let data_dir = dirs::data_dir()
            .ok_or_else(|| Error::Io("Kein Datenverzeichnis gefunden".into()))?
            .join(APP_DIR_NAME);
        Ok(Self {
            config_dir,
            data_dir,
        })
    }

    pub fn with_root(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.backup_dir())?;
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("yattt.sqlite")
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    pub fn secrets_fallback_file(&self) -> PathBuf {
        self.config_dir.join("secrets.json")
    }
}

pub fn load_settings(paths: &AppPaths) -> Result<Settings> {
    let file = paths.settings_file();
    if !file.exists() {
        return Ok(Settings::default());
    }
    let raw = std::fs::read_to_string(&file)?;
    let settings: Settings = serde_json::from_str(&raw)?;
    Ok(settings)
}

pub fn save_settings(paths: &AppPaths, settings: &Settings) -> Result<()> {
    settings.validate()?;
    paths.ensure()?;
    let tmp = paths.settings_file().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
    std::fs::rename(&tmp, paths.settings_file())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::with_root(dir.path());
        let mut s = Settings::default();
        s.jira.base_url = "https://jira.example.com/".into();
        s.selection.project_keys = vec!["ABC".into()];
        save_settings(&paths, &s).unwrap();
        let loaded = load_settings(&paths).unwrap();
        assert_eq!(loaded, s);
        assert_eq!(loaded.jira_base(), "https://jira.example.com");
    }

    #[test]
    fn invalid_url_rejected() {
        let mut s = Settings::default();
        s.jira.base_url = "ftp://x".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"jira":{"baseUrl":"https://j.example"}}"#).unwrap();
        assert_eq!(s.refresh.interval_minutes, 10);
        assert_eq!(s.selection.excluded_issue_types, vec!["Epic".to_string()]);
    }
}
