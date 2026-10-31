//! Jira REST client. Works against Jira Cloud and Jira Data Center with any of the supported
//! credentials. Authentication state is never persisted here; see [`crate::secrets`].

pub mod jql;
pub mod parse;

use crate::error::{Error, Result};
use crate::models::*;
use chrono::{DateTime, Local, NaiveDate, Utc};
use parse::{FieldIds, RemoteWorklog};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub const USER_AGENT: &str = concat!("YaTTT/", env!("CARGO_PKG_VERSION"));

/// Called whenever the server rotated session cookies, so the caller can persist them.
pub type CookieListener = Arc<dyn Fn(&[StoredCookie]) + Send + Sync>;

pub struct JiraClient {
    http: reqwest::Client,
    base: String,
    credential: RwLock<Credential>,
    deployment: RwLock<Deployment>,
    field_ids: RwLock<Option<FieldIds>>,
    cookie_listener: RwLock<Option<CookieListener>>,
}

/// Format a timestamp the way Jira worklogs expect it: `2026-10-30T09:00:00.000+0100`.
pub fn jira_timestamp(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&Local)
        .format("%Y-%m-%dT%H:%M:%S%.3f%z")
        .to_string()
}

/// Parse Jira's worklog timestamp format (also accepts RFC 3339).
pub fn parse_jira_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn cookie_header(cookies: &[StoredCookie]) -> String {
    cookies
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

impl JiraClient {
    pub fn new(base_url: &str, credential: Credential, deployment: Deployment) -> Result<Self> {
        let base = base_url.trim_end_matches('/').to_string();
        let parsed = url::Url::parse(&base)?;
        if parsed.host_str().is_none() {
            return Err(Error::InvalidUrl(base));
        }
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        // Non-form requests are exempt from XSRF checks when this header is present.
        headers.insert("X-Atlassian-Token", HeaderValue::from_static("no-check"));
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self {
            http,
            base,
            credential: RwLock::new(credential),
            deployment: RwLock::new(deployment),
            field_ids: RwLock::new(None),
            cookie_listener: RwLock::new(None),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn deployment(&self) -> Deployment {
        *self.deployment.read().unwrap()
    }

    pub fn credential_kind(&self) -> &'static str {
        self.credential.read().unwrap().kind()
    }

    /// Current cookies, if the client uses a cookie session.
    pub fn cookies(&self) -> Option<Vec<StoredCookie>> {
        match &*self.credential.read().unwrap() {
            Credential::Cookies { cookies } => Some(cookies.clone()),
            _ => None,
        }
    }

    pub fn set_cookie_listener(&self, listener: CookieListener) {
        *self.cookie_listener.write().unwrap() = Some(listener);
    }

    pub fn issue_url(&self, key: &str) -> String {
        format!("{}/browse/{}", self.base, key)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let rb = self.http.request(method, url);
        match &*self.credential.read().unwrap() {
            Credential::Cookies { cookies } => rb.header(COOKIE, cookie_header(cookies)),
            Credential::Basic { username, password } => rb.basic_auth(username, Some(password)),
            Credential::Bearer { token } => rb.bearer_auth(token),
            Credential::ApiToken { email, token } => rb.basic_auth(email, Some(token)),
        }
    }

    /// Sends the request, retries once on HTTP 429 (honouring `Retry-After`) and absorbs rotated
    /// session cookies from `Set-Cookie` headers.
    async fn execute(&self, rb: RequestBuilder) -> Result<Response> {
        let retry = rb.try_clone();
        let mut resp = rb.send().await?;
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            if let Some(rb2) = retry {
                let wait = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(5)
                    .min(30);
                tracing::warn!("Jira rate limit, retrying in {wait}s");
                tokio::time::sleep(Duration::from_secs(wait)).await;
                resp = rb2.send().await?;
            }
        }
        self.absorb_set_cookies(&resp);
        self.check(resp).await
    }

    fn absorb_set_cookies(&self, resp: &Response) {
        let headers = resp.headers().get_all(reqwest::header::SET_COOKIE);
        let mut updates: Vec<(String, String, bool)> = Vec::new();
        for h in headers.iter() {
            if let Ok(raw) = h.to_str() {
                if let Some((name, value)) = raw.split(';').next().and_then(|kv| kv.split_once('='))
                {
                    let lower = raw.to_lowercase();
                    let expired =
                        lower.contains("max-age=0") || lower.contains("expires=thu, 01 jan 1970");
                    updates.push((name.trim().to_string(), value.trim().to_string(), expired));
                }
            }
        }
        if updates.is_empty() {
            return;
        }
        let mut changed = false;
        let mut guard = self.credential.write().unwrap();
        if let Credential::Cookies { cookies } = &mut *guard {
            for (name, value, expired) in updates {
                if expired {
                    let before = cookies.len();
                    cookies.retain(|c| c.name != name);
                    changed |= cookies.len() != before;
                } else if let Some(c) = cookies.iter_mut().find(|c| c.name == name) {
                    if c.value != value {
                        c.value = value;
                        changed = true;
                    }
                } else {
                    cookies.push(StoredCookie {
                        name,
                        value,
                        domain: None,
                        path: Some("/".into()),
                        expires: None,
                        secure: true,
                        http_only: true,
                    });
                    changed = true;
                }
            }
            if changed {
                let snapshot = cookies.clone();
                drop(guard);
                if let Some(listener) = &*self.cookie_listener.read().unwrap() {
                    listener(&snapshot);
                }
            }
        }
    }

    async fn check(&self, resp: Response) -> Result<Response> {
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::AuthExpired(status.as_u16()));
        }
        if status.is_redirection() {
            // Cookie sessions that expired redirect to the login page.
            return Err(Error::AuthExpired(status.as_u16()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    let mut msgs: Vec<String> = v
                        .get("errorMessages")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(errors) = v.get("errors").and_then(Value::as_object) {
                        for (k, val) in errors {
                            msgs.push(format!("{k}: {}", val.as_str().unwrap_or("")));
                        }
                    }
                    if msgs.is_empty() {
                        None
                    } else {
                        Some(msgs.join("; "))
                    }
                })
                .unwrap_or_else(|| body.chars().take(300).collect());
            return Err(Error::Http {
                status: status.as_u16(),
                message,
            });
        }
        Ok(resp)
    }

    pub(crate) async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let resp = self
            .execute(self.request(Method::GET, path).query(query))
            .await?;
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp.text().await?;
        if !ct.contains("json") && !text.trim_start().starts_with(['{', '[']) {
            // HTML login page served with 200 -> treat as expired session.
            return Err(Error::AuthExpired(200));
        }
        Ok(serde_json::from_str(&text)?)
    }

    pub(crate) async fn send_json(
        &self,
        method: Method,
        path: &str,
        body: &Value,
    ) -> Result<Value> {
        let resp = self.execute(self.request(method, path).json(body)).await?;
        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let text = resp.text().await?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    pub(crate) async fn delete(&self, path: &str) -> Result<()> {
        self.execute(self.request(Method::DELETE, path)).await?;
        Ok(())
    }

    // ------------------------------------------------------------- basics

    pub async fn server_info(&self) -> Result<ServerInfo> {
        let v = self.get_json("rest/api/2/serverInfo", &[]).await?;
        let info = parse::parse_server_info(&v);
        if info.deployment != Deployment::Unknown {
            *self.deployment.write().unwrap() = info.deployment;
        }
        Ok(info)
    }

    pub async fn myself(&self) -> Result<JiraUser> {
        let v = self.get_json("rest/api/2/myself", &[]).await?;
        parse::parse_user(&v).ok_or_else(|| {
            Error::Parse(
                crate::tr!("Benutzerantwort unvollständig", "Incomplete user response").into(),
            )
        })
    }

    /// Checks the credentials and detects the deployment type.
    pub async fn verify(&self) -> Result<(JiraUser, ServerInfo)> {
        let user = self.myself().await?;
        let info = self.server_info().await?;
        Ok((user, info))
    }

    async fn ensure_deployment(&self) -> Result<Deployment> {
        if self.deployment() == Deployment::Unknown {
            self.server_info().await?;
        }
        Ok(self.deployment())
    }

    pub async fn field_ids(&self) -> Result<FieldIds> {
        if let Some(ids) = self.field_ids.read().unwrap().clone() {
            return Ok(ids);
        }
        let v = self.get_json("rest/api/2/field", &[]).await?;
        let ids = FieldIds::from_fields(&v);
        *self.field_ids.write().unwrap() = Some(ids.clone());
        Ok(ids)
    }

    // ----------------------------------------------------------- projects

    /// Projects matching `query` (key or name, case-insensitive). Empty query returns all (max 200).
    pub async fn search_projects(&self, query: &str) -> Result<Vec<Project>> {
        let deployment = self.ensure_deployment().await?;
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        if deployment == Deployment::Cloud {
            let mut start = 0;
            loop {
                let mut params = vec![
                    ("maxResults", "50".to_string()),
                    ("startAt", start.to_string()),
                    ("orderBy", "name".to_string()),
                ];
                if !q.is_empty() {
                    params.push(("query", query.trim().to_string()));
                }
                let v = self.get_json("rest/api/2/project/search", &params).await?;
                let values = v
                    .get("values")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let n = values.len();
                out.extend(values.iter().filter_map(parse::parse_project));
                let is_last = v.get("isLast").and_then(Value::as_bool).unwrap_or(true);
                if is_last || n == 0 || out.len() >= 200 {
                    break;
                }
                start += n;
            }
        } else {
            let v = self.get_json("rest/api/2/project", &[]).await?;
            let all = v.as_array().cloned().unwrap_or_default();
            out.extend(all.iter().filter_map(parse::parse_project).filter(|p| {
                q.is_empty()
                    || p.key.to_lowercase().contains(&q)
                    || p.name.to_lowercase().contains(&q)
            }));
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        out.truncate(200);
        Ok(out)
    }

    // ------------------------------------------------------ boards/sprints

    pub async fn boards_for_project(&self, project_key: &str) -> Result<Vec<Board>> {
        let mut out = Vec::new();
        let mut start = 0;
        loop {
            let v = self
                .get_json(
                    "rest/agile/1.0/board",
                    &[
                        ("projectKeyOrId", project_key.to_string()),
                        ("maxResults", "50".into()),
                        ("startAt", start.to_string()),
                    ],
                )
                .await?;
            let values = v
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let n = values.len();
            out.extend(values.iter().filter_map(parse::parse_board));
            let is_last = v.get("isLast").and_then(Value::as_bool).unwrap_or(true);
            if is_last || n == 0 {
                break;
            }
            start += n;
        }
        Ok(out)
    }

    /// Sprints of a board. Kanban boards have no sprints; Jira answers 400, which is mapped to an empty list.
    pub async fn sprints_for_board(&self, board_id: i64, states: &str) -> Result<Vec<Sprint>> {
        let mut out = Vec::new();
        let mut start = 0;
        loop {
            let res = self
                .get_json(
                    &format!("rest/agile/1.0/board/{board_id}/sprint"),
                    &[
                        ("state", states.to_string()),
                        ("maxResults", "50".into()),
                        ("startAt", start.to_string()),
                    ],
                )
                .await;
            let v = match res {
                Ok(v) => v,
                Err(Error::Http { status: 400, .. }) | Err(Error::Http { status: 404, .. }) => {
                    break
                }
                Err(e) => return Err(e),
            };
            let values = v
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let n = values.len();
            out.extend(values.iter().filter_map(parse::parse_sprint).map(|mut s| {
                s.board_id.get_or_insert(board_id);
                s
            }));
            let is_last = v.get("isLast").and_then(Value::as_bool).unwrap_or(true);
            if is_last || n == 0 {
                break;
            }
            start += n;
        }
        Ok(out)
    }

    /// All active and future sprints of all boards of the given projects, filtered by `query`.
    pub async fn search_sprints(
        &self,
        project_keys: &[String],
        query: &str,
        include_closed: bool,
    ) -> Result<Vec<Sprint>> {
        let states = if include_closed {
            "active,future,closed"
        } else {
            "active,future"
        };
        let q = query.trim().to_lowercase();
        let mut out: Vec<Sprint> = Vec::new();
        for key in project_keys {
            let boards = self.boards_for_project(key).await?;
            for board in boards.into_iter().filter(|b| b.board_type != "kanban") {
                for sprint in self.sprints_for_board(board.id, states).await? {
                    if out.iter().any(|s| s.id == sprint.id) {
                        continue;
                    }
                    if q.is_empty() || sprint.name.to_lowercase().contains(&q) {
                        out.push(sprint);
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            state_rank(&a.state)
                .cmp(&state_rank(&b.state))
                .then_with(|| b.start_date.cmp(&a.start_date))
        });
        Ok(out)
    }

    /// Label suggestions for the settings dialog. Uses the JQL autocomplete endpoint, which
    /// exists on Cloud and Data Center and returns site-wide labels matching `query`.
    pub async fn search_labels(&self, query: &str) -> Result<Vec<String>> {
        let v = self
            .get_json(
                "/rest/api/2/jql/autocompletedata/suggestions",
                &[
                    ("fieldName", "labels".to_string()),
                    ("fieldValue", query.trim().to_string()),
                ],
            )
            .await?;
        let mut out: Vec<String> = v
            .get("results")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("value").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// Component names of a project (Cloud and Data Center), sorted.
    pub async fn project_components(&self, project_key: &str) -> Result<Vec<String>> {
        let v = self
            .get_json(
                &format!("/rest/api/2/project/{project_key}/components"),
                &[],
            )
            .await?;
        let mut out: Vec<String> = v
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|s| s.to_lowercase());
        out.dedup();
        Ok(out)
    }

    pub async fn issue_types(&self) -> Result<Vec<IssueType>> {
        let v = self.get_json("rest/api/2/issuetype", &[]).await?;
        let mut types: Vec<IssueType> = v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(parse::parse_issue_type)
            .collect();
        // de-duplicate by name (Cloud has one type per project in team-managed projects)
        types.sort_by(|a, b| a.name.cmp(&b.name));
        types.dedup_by(|a, b| a.name == b.name);
        Ok(types)
    }

    // -------------------------------------------------------------- issues

    /// Runs a JQL search and returns up to `max_total` issues.
    pub async fn search_issues(&self, jql: &str, max_total: usize) -> Result<Vec<Issue>> {
        let deployment = self.ensure_deployment().await?;
        let ids = self.field_ids().await?;
        let fields = ids.search_fields();
        let mut out = Vec::new();
        if deployment == Deployment::Cloud {
            let mut token: Option<String> = None;
            loop {
                let mut params = vec![
                    ("jql", jql.to_string()),
                    ("fields", fields.clone()),
                    ("maxResults", "100".to_string()),
                ];
                if let Some(t) = &token {
                    params.push(("nextPageToken", t.clone()));
                }
                let v = self.get_json("rest/api/2/search/jql", &params).await?;
                let issues = v
                    .get("issues")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                out.extend(issues.iter().filter_map(|i| parse::parse_issue(i, &ids)));
                token = v
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let is_last = v
                    .get("isLast")
                    .and_then(Value::as_bool)
                    .unwrap_or(token.is_none());
                if is_last || token.is_none() || out.len() >= max_total {
                    break;
                }
            }
        } else {
            let mut start = 0usize;
            loop {
                let params = vec![
                    ("jql", jql.to_string()),
                    ("fields", fields.clone()),
                    ("maxResults", "100".to_string()),
                    ("startAt", start.to_string()),
                ];
                let v = self.get_json("rest/api/2/search", &params).await?;
                let issues = v
                    .get("issues")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let n = issues.len();
                out.extend(issues.iter().filter_map(|i| parse::parse_issue(i, &ids)));
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                start += n;
                if n == 0 || start >= total || out.len() >= max_total {
                    break;
                }
            }
        }
        out.truncate(max_total);
        Ok(out)
    }

    pub async fn get_issue(&self, key: &str) -> Result<Issue> {
        let ids = self.field_ids().await?;
        let v = self
            .get_json(
                &format!("rest/api/2/issue/{key}"),
                &[("fields", ids.search_fields())],
            )
            .await?;
        parse::parse_issue(&v, &ids).ok_or_else(|| {
            Error::Parse(crate::tr!(
                "Ticket {key} konnte nicht gelesen werden",
                "Ticket {key} could not be read",
                key = key
            ))
        })
    }

    // ------------------------------------------------------------ worklogs

    pub async fn add_worklog(
        &self,
        issue_key: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        comment: &str,
    ) -> Result<String> {
        let body = json!({
            "started": jira_timestamp(started),
            "timeSpentSeconds": seconds.max(60),
            "comment": comment,
        });
        let v = self
            .send_json(
                Method::POST,
                &format!("rest/api/2/issue/{issue_key}/worklog?adjustEstimate=auto"),
                &body,
            )
            .await?;
        v.get("id")
            .map(|i| {
                i.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| i.to_string())
            })
            .ok_or_else(|| {
                Error::Parse(
                    crate::tr!("Worklog-Antwort ohne id", "Worklog response without id").into(),
                )
            })
    }

    pub async fn update_worklog(
        &self,
        issue_key: &str,
        worklog_id: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        comment: &str,
    ) -> Result<()> {
        let body = json!({
            "started": jira_timestamp(started),
            "timeSpentSeconds": seconds.max(60),
            "comment": comment,
        });
        self.send_json(
            Method::PUT,
            &format!("rest/api/2/issue/{issue_key}/worklog/{worklog_id}?adjustEstimate=auto"),
            &body,
        )
        .await?;
        Ok(())
    }

    pub async fn delete_worklog(&self, issue_key: &str, worklog_id: &str) -> Result<()> {
        match self
            .delete(&format!(
                "rest/api/2/issue/{issue_key}/worklog/{worklog_id}?adjustEstimate=auto"
            ))
            .await
        {
            Ok(()) => Ok(()),
            Err(Error::Http { status: 404, .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Worklogs of `user` between `from` and `to` (inclusive, local dates).
    pub async fn my_worklogs(
        &self,
        user: &JiraUser,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<RemoteWorklog>> {
        let jql = format!(
            "worklogAuthor = currentUser() AND worklogDate >= \"{}\" AND worklogDate <= \"{}\" ORDER BY updated DESC",
            from.format("%Y-%m-%d"),
            to.format("%Y-%m-%d")
        );
        let issues = self.search_issues(&jql, 500).await?;
        let mut out = Vec::new();
        for issue in issues {
            let mut start = 0usize;
            loop {
                let v = self
                    .get_json(
                        &format!("rest/api/2/issue/{}/worklog", issue.key),
                        &[
                            ("startAt", start.to_string()),
                            ("maxResults", "1000".to_string()),
                        ],
                    )
                    .await?;
                let logs = v
                    .get("worklogs")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let n = logs.len();
                for w in logs
                    .iter()
                    .filter_map(|w| parse::parse_worklog(w, &issue.key))
                {
                    if w.author_id.as_deref() != Some(user.id.as_str()) {
                        continue;
                    }
                    if let Some(started) = parse_jira_timestamp(&w.started) {
                        let d = started.with_timezone(&Local).date_naive();
                        if d >= from && d <= to {
                            out.push(w);
                        }
                    }
                }
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                start += n;
                if n == 0 || start >= total {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Authenticate against Data Center with username/password and return the session cookie.
    pub async fn login_with_password(
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Vec<StoredCookie>> {
        let base = base_url.trim_end_matches('/');
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        let resp = http
            .post(format!("{base}/rest/auth/1/session"))
            .header("X-Atlassian-Token", "no-check")
            .json(&json!({"username": username, "password": password}))
            .send()
            .await?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Invalid(
                crate::tr!(
                    "Benutzername oder Passwort falsch",
                    "Wrong username or password"
                )
                .into(),
            ));
        }
        if status == StatusCode::FORBIDDEN {
            let reason = resp
                .headers()
                .get("X-Seraph-LoginReason")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if reason.contains("AUTHENTICATION_DENIED") {
                return Err(Error::Invalid(
                    crate::tr!(
                        "Jira verlangt ein CAPTCHA. Bitte einmal im Browser anmelden oder die Browser-Anmeldung verwenden.",
                        "Jira requires a CAPTCHA. Sign in once in the browser or use the browser login."
                    )
                    .into(),
                ));
            }
            return Err(Error::AuthExpired(403));
        }
        if !status.is_success() {
            return Err(Error::Http {
                status: status.as_u16(),
                message: crate::tr!("Anmeldung fehlgeschlagen", "Sign-in failed").into(),
            });
        }
        let mut cookies = Vec::new();
        for header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(raw) = header.to_str() {
                if let Some((name, value)) = raw.split(';').next().and_then(|kv| kv.split_once('='))
                {
                    cookies.push(StoredCookie {
                        name: name.trim().to_string(),
                        value: value.trim().to_string(),
                        domain: None,
                        path: Some("/".into()),
                        expires: None,
                        secure: raw.to_lowercase().contains("secure"),
                        http_only: raw.to_lowercase().contains("httponly"),
                    });
                }
            }
        }
        if cookies.is_empty() {
            // Older instances put the session into the JSON body.
            let v: Value = resp.json().await.unwrap_or(Value::Null);
            if let (Some(name), Some(value)) = (
                v.pointer("/session/name").and_then(Value::as_str),
                v.pointer("/session/value").and_then(Value::as_str),
            ) {
                cookies.push(StoredCookie {
                    name: name.into(),
                    value: value.into(),
                    domain: None,
                    path: Some("/".into()),
                    expires: None,
                    secure: true,
                    http_only: true,
                });
            }
        }
        if cookies.is_empty() {
            return Err(Error::Parse(
                crate::tr!(
                    "Keine Session-Cookies in der Antwort",
                    "No session cookies in the response"
                )
                .into(),
            ));
        }
        Ok(cookies)
    }
}

fn state_rank(state: &str) -> u8 {
    match state {
        "active" => 0,
        "future" => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cookie_cred() -> Credential {
        Credential::Cookies {
            cookies: vec![StoredCookie {
                name: "JSESSIONID".into(),
                value: "abc123".into(),
                domain: None,
                path: None,
                expires: None,
                secure: true,
                http_only: true,
            }],
        }
    }

    #[tokio::test]
    async fn sends_cookie_and_xsrf_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .and(header("Cookie", "JSESSIONID=abc123"))
            .and(header("X-Atlassian-Token", "no-check"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"name": "sam", "displayName": "Sam"})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        let me = client.myself().await.unwrap();
        assert_eq!(me.id, "sam");
    }

    #[tokio::test]
    async fn rotated_cookies_are_absorbed_and_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "JSESSIONID=rotated; Path=/; HttpOnly")
                    .append_header("Set-Cookie", "atlassian.xsrf.token=new; Path=/")
                    .set_body_json(json!({"name": "sam", "displayName": "Sam"})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        let seen: Arc<std::sync::Mutex<Vec<StoredCookie>>> = Arc::default();
        let seen2 = seen.clone();
        client.set_cookie_listener(Arc::new(move |c| *seen2.lock().unwrap() = c.to_vec()));
        client.myself().await.unwrap();
        let cookies = client.cookies().unwrap();
        assert_eq!(
            cookies
                .iter()
                .find(|c| c.name == "JSESSIONID")
                .unwrap()
                .value,
            "rotated"
        );
        assert_eq!(cookies.len(), 2);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rate_limit_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/serverInfo"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/serverInfo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"baseUrl": "x", "version": "9", "deploymentType": "Server"}),
                ),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::Unknown).unwrap();
        let info = client.server_info().await.unwrap();
        assert_eq!(info.deployment, Deployment::DataCenter);
        assert_eq!(client.deployment(), Deployment::DataCenter);
    }

    #[tokio::test]
    async fn expired_session_is_detected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/login.jsp"))
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        assert!(matches!(
            client.myself().await,
            Err(Error::AuthExpired(302))
        ));

        Mock::given(method("GET"))
            .and(path("/rest/api/2/serverInfo"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        assert!(matches!(
            client.server_info().await,
            Err(Error::AuthExpired(401))
        ));
    }

    #[tokio::test]
    async fn label_suggestions_come_from_autocomplete() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/jql/autocompletedata/suggestions"))
            .and(query_param("fieldName", "labels"))
            .and(query_param("fieldValue", "team"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [
                {"value": "team-rot", "displayName": "<b>team</b>-rot"},
                {"value": "team-blau", "displayName": "<b>team</b>-blau"},
                {"value": "team-blau", "displayName": "<b>team</b>-blau"}
            ]})))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let labels = client.search_labels(" team ").await.unwrap();
        assert_eq!(
            labels,
            vec!["team-blau".to_string(), "team-rot".to_string()]
        );
    }

    #[tokio::test]
    async fn project_components_are_listed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/project/SHARED/components"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": "10001", "name": "Time Tracking", "description": "Zeiterfassung"},
                {"id": "10002", "name": "Payroll"}
            ])))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::Cloud,
        )
        .unwrap();
        let components = client.project_components("SHARED").await.unwrap();
        assert_eq!(
            components,
            vec!["Payroll".to_string(), "Time Tracking".to_string()]
        );
    }

    #[tokio::test]
    async fn dc_search_paginates_with_start_at() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let issue = |k: &str| json!({"id": "1", "key": k, "fields": {"summary": k, "project": {"key": "A"}, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}});
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search"))
            .and(query_param("startAt", "0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("A-1")], "total": 2, "startAt": 0})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search"))
            .and(query_param("startAt", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("A-2")], "total": 2, "startAt": 1})),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let issues = client.search_issues("project = A", 100).await.unwrap();
        assert_eq!(
            issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
            vec!["A-1", "A-2"]
        );
    }

    #[tokio::test]
    async fn cloud_search_uses_next_page_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let issue = |k: &str| json!({"id": "1", "key": k, "fields": {"summary": k, "issuetype": {"name": "Task"}, "status": {"name": "Open", "statusCategory": {"key": "new"}}}});
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search/jql"))
            .and(query_param("nextPageToken", "p2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"issues": [issue("C-2")], "isLast": true})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"issues": [issue("C-1")], "nextPageToken": "p2", "isLast": false}),
            ))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::ApiToken {
                email: "e".into(),
                token: "t".into(),
            },
            Deployment::Cloud,
        )
        .unwrap();
        let issues = client.search_issues("project = C", 100).await.unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[1].project_key, "C");
    }

    #[tokio::test]
    async fn worklog_create_and_error_mapping() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "777"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/2/issue/A-2/worklog"))
            .respond_with(ResponseTemplate::new(400).set_body_json(
                json!({"errorMessages": ["Worklog must not be negative"], "errors": {}}),
            ))
            .mount(&server)
            .await;
        let client = JiraClient::new(
            &server.uri(),
            Credential::Basic {
                username: "u".into(),
                password: "p".into(),
            },
            Deployment::DataCenter,
        )
        .unwrap();
        let id = client
            .add_worklog("A-1", &Utc::now(), 1800, "note")
            .await
            .unwrap();
        assert_eq!(id, "777");
        let err = client
            .add_worklog("A-2", &Utc::now(), 1800, "note")
            .await
            .unwrap_err();
        match err {
            Error::Http { status, message } => {
                assert_eq!(status, 400);
                assert!(message.contains("negative"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn password_login_collects_cookies() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/auth/1/session"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "JSESSIONID=XYZ; Path=/; HttpOnly")
                    .append_header("Set-Cookie", "atlassian.xsrf.token=TOK; Path=/")
                    .set_body_json(json!({"session": {"name": "JSESSIONID", "value": "XYZ"}})),
            )
            .mount(&server)
            .await;
        let cookies = JiraClient::login_with_password(&server.uri(), "u", "p")
            .await
            .unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "JSESSIONID");
        assert!(cookies[0].http_only);
    }

    #[tokio::test]
    async fn kanban_board_without_sprints_is_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/5/sprint"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(
                    json!({"errorMessages": ["The board does not support sprints"]}),
                ),
            )
            .mount(&server)
            .await;
        let client = JiraClient::new(&server.uri(), cookie_cred(), Deployment::DataCenter).unwrap();
        assert!(client
            .sprints_for_board(5, "active,future")
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn timestamp_roundtrip() {
        let now = Utc::now();
        let s = jira_timestamp(&now);
        let parsed = parse_jira_timestamp(&s).unwrap();
        assert_eq!(parsed.timestamp(), now.timestamp());
        assert!(parse_jira_timestamp("2026-10-30T09:00:00.000+0100").is_some());
    }
}
