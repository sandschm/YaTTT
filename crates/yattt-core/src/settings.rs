//! Application settings. Stored as JSON in the user's config directory.
//! Secrets never live here, they go to [`crate::secrets`].

use crate::error::{Error, Result};
use crate::i18n::Lang;
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
    /// Project key -> filter. For a project with a filter only issues matching it are loaded
    /// (projects shared by several teams). Projects without an entry are unfiltered.
    pub project_filters: BTreeMap<String, ProjectFilter>,
    /// Pre-0.3 format (labels only); migrated into `project_filters` on load.
    #[serde(skip_serializing)]
    pub project_labels: BTreeMap<String, Vec<String>>,
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
            project_filters: BTreeMap::new(),
            project_labels: BTreeMap::new(),
        }
    }
}

/// Per-project restriction by labels and/or components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectFilter {
    /// Issue must carry at least one of these labels (empty = no label condition).
    pub labels: Vec<String>,
    /// Issue must have at least one of these components (empty = no component condition).
    pub components: Vec<String>,
    /// With both lists set: `true` = labels AND components must match, `false` = one of them is enough.
    pub match_all: bool,
}

impl Default for ProjectFilter {
    fn default() -> Self {
        Self {
            labels: vec![],
            components: vec![],
            match_all: true,
        }
    }
}

fn clean_list(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for x in list {
        let x = x.trim();
        if !x.is_empty() && !out.iter().any(|y| y == x) {
            out.push(x.to_string());
        }
    }
    out
}

impl ProjectFilter {
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() && self.components.is_empty()
    }

    fn cleaned(&self) -> ProjectFilter {
        ProjectFilter {
            labels: clean_list(&self.labels),
            components: clean_list(&self.components),
            match_all: self.match_all,
        }
    }
}

impl SelectionSettings {
    /// The cleaned filter of `project_key`, if it restricts anything.
    pub fn filter_for(&self, project_key: &str) -> Option<ProjectFilter> {
        self.project_filters
            .get(project_key)
            .map(ProjectFilter::cleaned)
            .filter(|f| !f.is_empty())
    }

    /// Moves the pre-0.3 label lists into `project_filters`.
    pub fn migrate_legacy(&mut self) {
        let legacy = std::mem::take(&mut self.project_labels);
        for (key, labels) in legacy {
            let entry = self.project_filters.entry(key).or_default();
            if entry.labels.is_empty() {
                entry.labels = labels;
            }
        }
    }

    /// Drops filters of projects that are no longer selected, empty entries and whitespace.
    pub fn tidy_project_filters(&mut self) {
        self.migrate_legacy();
        let keys = self.project_keys.clone();
        let cleaned: BTreeMap<String, ProjectFilter> = self
            .project_filters
            .iter()
            .filter(|(k, _)| keys.contains(k))
            .map(|(k, f)| (k.clone(), f.cleaned()))
            .filter(|(_, f)| !f.is_empty())
            .collect();
        self.project_filters = cleaned;
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

/// Language of the user interface, tray menu, notifications and messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    /// Follow the operating system (German when it is German, English otherwise).
    #[default]
    System,
    De,
    En,
}

impl Language {
    /// Resolves the choice to a concrete language; `system_tag` is the OS locale (e.g. `de-DE`).
    pub fn resolve(self, system_tag: Option<&str>) -> Lang {
        match self {
            Language::De => Lang::De,
            Language::En => Lang::En,
            Language::System => system_tag.map(Lang::from_tag).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BehaviorSettings {
    pub close_to_tray: bool,
    pub autostart: bool,
    pub notifications: bool,
    pub theme: Theme,
    pub language: Language,
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
            language: Language::System,
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
        if !self.jira.base_url.trim().is_empty() {
            normalize_jira_url(&self.jira.base_url)?;
        }
        if self.refresh.interval_minutes == 0 && self.refresh.auto {
            return Err(Error::Invalid(
                crate::tr!(
                    "Aktualisierungsintervall muss mindestens 1 Minute sein",
                    "The refresh interval must be at least 1 minute"
                )
                .into(),
            ));
        }
        check_extra_jql(&self.selection.extra_jql)?;
        Ok(())
    }

    /// Normalized base URL without trailing slash (see [`normalize_jira_url`]).
    pub fn jira_base(&self) -> String {
        normalize_jira_url(&self.jira.base_url)
            .unwrap_or_else(|_| self.jira.base_url.trim_end_matches('/').to_string())
    }
}

/// Turns whatever the user pasted into the REST base URL of the Jira instance.
///
/// * Jira Cloud (`*.atlassian.net`, `*.jira.com`): the API always lives at the origin, UI paths
///   such as `/jira/software/...` or `/jira/your-work` are dropped.
/// * Data Center: UI paths (`/browse/KEY`, `/secure/...`, `/projects/...`, `/issues/...`,
///   `/plugins/...`, `/rest/...`) are dropped, a context path like `/jira` is kept.
pub fn normalize_jira_url(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = url::Url::parse(&with_scheme)?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(Error::InvalidUrl(
            crate::tr!("Nur http(s) wird unterstützt", "Only http(s) is supported").into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| {
            Error::InvalidUrl(crate::tr!("Kein Host in der URL", "The URL has no host").into())
        })?
        .to_lowercase();
    url.set_query(None);
    url.set_fragment(None);
    let is_cloud = host.ends_with(".atlassian.net") || host.ends_with(".jira.com");
    let path = if is_cloud {
        String::new()
    } else {
        const UI_MARKERS: &[&str] = &[
            "/browse/",
            "/secure/",
            "/projects/",
            "/issues/",
            "/plugins/",
            "/rest/",
            "/login.jsp",
            "/jira/software/",
            "/servicedesk/",
        ];
        let mut p = url.path().to_string();
        let lower = p.to_lowercase();
        let cut = UI_MARKERS.iter().filter_map(|m| lower.find(m)).min();
        if let Some(i) = cut {
            p.truncate(i);
        }
        p.trim_end_matches('/').to_string()
    };
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_string())
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
            .ok_or_else(|| {
                Error::Io(
                    crate::tr!(
                        "Kein Konfigurationsverzeichnis gefunden",
                        "No config directory found"
                    )
                    .into(),
                )
            })?
            .join(APP_DIR_NAME);
        let data_dir = dirs::data_dir()
            .ok_or_else(|| {
                Error::Io(
                    crate::tr!("Kein Datenverzeichnis gefunden", "No data directory found").into(),
                )
            })?
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
    let mut settings: Settings = serde_json::from_str(&raw)?;
    settings.selection.migrate_legacy();
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

/// A single bare word ("Tim") is never a valid JQL clause; Jira answers HTTP 400 for
/// `AND (Tim)`. Catch it when saving and point at the label filter instead.
pub fn check_extra_jql(extra: &str) -> Result<()> {
    let extra = extra.trim();
    if extra.is_empty() {
        return Ok(());
    }
    let has_operator = extra.contains(['=', '~', '<', '>', '!', '(', ')']);
    if !has_operator && !extra.contains(char::is_whitespace) {
        return Err(Error::Invalid(crate::tr!(
            "„{extra}“ ist keine JQL-Bedingung. Für ein Label: labels = {extra}. \
             Labels je Projekt ordnest du unter „Projekte & Sprints“ zu.",
            "\"{extra}\" is not a JQL condition. For a label use: labels = {extra}. \
             Labels per project are assigned under \"Projects & Sprints\".",
            extra = extra
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_project_labels_migrate_into_filters() {
        let raw =
            r#"{"selection":{"projectKeys":["SHARED"],"projectLabels":{"SHARED":["team-blau"]}}}"#;
        let mut settings: Settings = serde_json::from_str(raw).unwrap();
        settings.selection.migrate_legacy();
        let f = settings.selection.filter_for("SHARED").unwrap();
        assert_eq!(f.labels, vec!["team-blau".to_string()]);
        assert!(f.components.is_empty());
        assert!(f.match_all);
        assert!(settings.selection.project_labels.is_empty());
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("projectLabels"));
        assert!(json.contains("projectFilters"));
    }

    #[test]
    fn bare_word_is_not_accepted_as_extra_jql() {
        assert!(check_extra_jql("").is_ok());
        assert!(check_extra_jql("labels = Tim").is_ok());
        assert!(check_extra_jql("labels in (a, b)").is_ok());
        assert!(check_extra_jql("assignee is empty").is_ok());
        let err = check_extra_jql(" Tim ").unwrap_err().to_string();
        assert!(err.contains("labels = Tim"), "{err}");
        let mut settings = Settings::default();
        settings.selection.extra_jql = "Tim".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn language_resolves_system_locale() {
        assert_eq!(Language::System.resolve(Some("de-DE")), Lang::De);
        assert_eq!(Language::System.resolve(Some("en-US")), Lang::En);
        assert_eq!(Language::System.resolve(None), Lang::En);
        assert_eq!(Language::De.resolve(Some("en-US")), Lang::De);
        assert_eq!(Language::En.resolve(Some("de-DE")), Lang::En);
        let s: Settings = serde_json::from_str(r#"{"behavior":{"language":"en"}}"#).unwrap();
        assert_eq!(s.behavior.language, Language::En);
        assert_eq!(Settings::default().behavior.language, Language::System);
    }

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
    fn normalizes_cloud_and_dc_urls() {
        assert_eq!(
            normalize_jira_url("https://acme.atlassian.net/jira").unwrap(),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("https://x.atlassian.net/jira/software/projects/ABC/boards/1?x=1")
                .unwrap(),
            "https://x.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("x.atlassian.net").unwrap(),
            "https://x.atlassian.net"
        );
        assert_eq!(
            normalize_jira_url("https://jira.firma.de/browse/ABC-1").unwrap(),
            "https://jira.firma.de"
        );
        assert_eq!(
            normalize_jira_url("https://tools.firma.de/jira/secure/Dashboard.jspa").unwrap(),
            "https://tools.firma.de/jira"
        );
        assert_eq!(
            normalize_jira_url("https://tools.firma.de/jira/").unwrap(),
            "https://tools.firma.de/jira"
        );
        assert_eq!(
            normalize_jira_url("https://jira.firma.de/rest/api/2/myself").unwrap(),
            "https://jira.firma.de"
        );
        assert_eq!(normalize_jira_url("  ").unwrap(), "");
        assert!(normalize_jira_url("ftp://jira.firma.de").is_err());
        let mut s = Settings::default();
        s.jira.base_url = "https://acme.atlassian.net/jira".into();
        assert_eq!(s.jira_base(), "https://acme.atlassian.net");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"jira":{"baseUrl":"https://j.example"}}"#).unwrap();
        assert_eq!(s.refresh.interval_minutes, 10);
        assert_eq!(s.selection.excluded_issue_types, vec!["Epic".to_string()]);
    }
}
