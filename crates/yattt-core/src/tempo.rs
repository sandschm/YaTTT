//! Tempo integration.
//!
//! * Tempo Data Center stores worklogs in Jira; `/rest/tempo-timesheets/4/worklogs` is reachable
//!   with the Jira session and allows Tempo attributes such as billable seconds.
//! * Tempo Cloud has its own API (`https://api.tempo.io/4`) with a Tempo token. Worklogs created
//!   through the plain Jira API are only mirrored into Tempo Cloud overnight, so the Tempo API is
//!   the recommended path for Cloud (plan review, finding 3).

use crate::error::{Error, Result};
use crate::jira::JiraClient;
use chrono::{DateTime, Local, Utc};
use reqwest::Method;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoWorklogIds {
    pub tempo_worklog_id: String,
    /// Jira worklog id created by Tempo, when reported.
    pub jira_worklog_id: Option<String>,
}

fn tempo_dc_started(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

/// Tempo Timesheets for Jira Data Center, using the Jira client's session.
pub struct TempoDataCenter<'a> {
    pub jira: &'a JiraClient,
}

impl<'a> TempoDataCenter<'a> {
    pub async fn create_worklog(
        &self,
        issue_key: &str,
        worker_key: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        billable_seconds: i64,
        comment: &str,
    ) -> Result<TempoWorklogIds> {
        let body = json!({
            "attributes": {},
            "billableSeconds": billable_seconds,
            "originId": -1,
            "worker": worker_key,
            "comment": comment,
            "started": tempo_dc_started(started),
            "timeSpentSeconds": seconds.max(60),
            "originTaskId": issue_key,
            "remainingEstimate": null,
            "endDate": null,
            "includeNonWorkingDays": false
        });
        let v = self
            .jira
            .send_json(Method::POST, "rest/tempo-timesheets/4/worklogs/", &body)
            .await?;
        let first = match &v {
            Value::Array(items) => items.first().cloned().unwrap_or(Value::Null),
            other => other.clone(),
        };
        let tempo_id = first
            .get("tempoWorklogId")
            .or_else(|| first.get("id"))
            .map(value_to_string)
            .ok_or_else(|| {
                Error::Parse(
                    crate::tr!(
                        "Tempo-Antwort ohne tempoWorklogId",
                        "Tempo response without tempoWorklogId"
                    )
                    .into(),
                )
            })?;
        let jira_id = first
            .get("originId")
            .map(value_to_string)
            .filter(|s| s != "-1" && !s.is_empty());
        Ok(TempoWorklogIds {
            tempo_worklog_id: tempo_id,
            jira_worklog_id: jira_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_worklog(
        &self,
        tempo_worklog_id: &str,
        issue_key: &str,
        worker_key: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        billable_seconds: i64,
        comment: &str,
    ) -> Result<()> {
        let body = json!({
            "attributes": {},
            "billableSeconds": billable_seconds,
            "worker": worker_key,
            "comment": comment,
            "started": tempo_dc_started(started),
            "timeSpentSeconds": seconds.max(60),
            "originTaskId": issue_key,
            "includeNonWorkingDays": false
        });
        self.jira
            .send_json(
                Method::PUT,
                &format!("rest/tempo-timesheets/4/worklogs/{tempo_worklog_id}"),
                &body,
            )
            .await?;
        Ok(())
    }

    pub async fn delete_worklog(&self, tempo_worklog_id: &str) -> Result<()> {
        match self
            .jira
            .delete(&format!(
                "rest/tempo-timesheets/4/worklogs/{tempo_worklog_id}"
            ))
            .await
        {
            Ok(()) | Err(Error::Http { status: 404, .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// Tempo Cloud REST API v4.
pub struct TempoCloud {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl TempoCloud {
    pub fn new(base: &str, token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::jira::USER_AGENT)
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    async fn send(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let mut rb = self
            .http
            .request(
                method,
                format!("{}/{}", self.base, path.trim_start_matches('/')),
            )
            .bearer_auth(&self.token)
            .header("Accept", "application/json");
        if let Some(b) = body {
            rb = rb.json(b);
        }
        let resp = rb.send().await?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(Error::AuthExpired(status.as_u16()));
        }
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("errors").cloned())
                .map(|e| e.to_string())
                .unwrap_or_else(|| text.chars().take(300).collect());
            return Err(Error::Http {
                status: status.as_u16(),
                message,
            });
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// `issue_id` must be the numeric Jira issue id (Tempo Cloud no longer accepts keys).
    pub async fn create_worklog(
        &self,
        issue_id: i64,
        author_account_id: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        billable_seconds: i64,
        description: &str,
    ) -> Result<TempoWorklogIds> {
        let local = started.with_timezone(&Local);
        let body = json!({
            "issueId": issue_id,
            "timeSpentSeconds": seconds.max(60),
            "billableSeconds": billable_seconds,
            "startDate": local.format("%Y-%m-%d").to_string(),
            "startTime": local.format("%H:%M:%S").to_string(),
            "description": if description.is_empty() { "Working on issue" } else { description },
            "authorAccountId": author_account_id,
        });
        let v = self.send(Method::POST, "worklogs", Some(&body)).await?;
        let tempo_id = v
            .get("tempoWorklogId")
            .map(value_to_string)
            .ok_or_else(|| {
                Error::Parse(
                    crate::tr!(
                        "Tempo-Antwort ohne tempoWorklogId",
                        "Tempo response without tempoWorklogId"
                    )
                    .into(),
                )
            })?;
        let jira_id = v.pointer("/issue/id").map(|_| ()).and(None); // Tempo Cloud does not expose the Jira worklog id here
        Ok(TempoWorklogIds {
            tempo_worklog_id: tempo_id,
            jira_worklog_id: jira_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_worklog(
        &self,
        tempo_worklog_id: &str,
        issue_id: i64,
        author_account_id: &str,
        started: &DateTime<Utc>,
        seconds: i64,
        billable_seconds: i64,
        description: &str,
    ) -> Result<()> {
        let local = started.with_timezone(&Local);
        let body = json!({
            "issueId": issue_id,
            "timeSpentSeconds": seconds.max(60),
            "billableSeconds": billable_seconds,
            "startDate": local.format("%Y-%m-%d").to_string(),
            "startTime": local.format("%H:%M:%S").to_string(),
            "description": if description.is_empty() { "Working on issue" } else { description },
            "authorAccountId": author_account_id,
        });
        self.send(
            Method::PUT,
            &format!("worklogs/{tempo_worklog_id}"),
            Some(&body),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_worklog(&self, tempo_worklog_id: &str) -> Result<()> {
        match self
            .send(
                Method::DELETE,
                &format!("worklogs/{tempo_worklog_id}"),
                None,
            )
            .await
        {
            Ok(_) | Err(Error::Http { status: 404, .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Checks the token by fetching the first worklog page.
    pub async fn verify(&self) -> Result<()> {
        self.send(Method::GET, "worklogs?limit=1", None).await?;
        Ok(())
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Credential, Deployment};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tempo_dc_create_returns_ids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/tempo-timesheets/4/worklogs/"))
            .and(body_partial_json(
                json!({"originTaskId": "ABC-1", "worker": "JIRAUSER1", "billableSeconds": 1800}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"tempoWorklogId": 55, "originId": 909}])),
            )
            .mount(&server)
            .await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let tempo = TempoDataCenter { jira: &jira };
        let ids = tempo
            .create_worklog("ABC-1", "JIRAUSER1", &Utc::now(), 1800, 1800, "c")
            .await
            .unwrap();
        assert_eq!(ids.tempo_worklog_id, "55");
        assert_eq!(ids.jira_worklog_id.as_deref(), Some("909"));
    }

    #[tokio::test]
    async fn tempo_cloud_create_sends_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/worklogs"))
            .and(header("Authorization", "Bearer tok"))
            .and(body_partial_json(
                json!({"issueId": 10001, "authorAccountId": "acc"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"tempoWorklogId": 1234})))
            .mount(&server)
            .await;
        let tempo = TempoCloud::new(&server.uri(), "tok").unwrap();
        let ids = tempo
            .create_worklog(10001, "acc", &Utc::now(), 600, 600, "")
            .await
            .unwrap();
        assert_eq!(ids.tempo_worklog_id, "1234");
    }
}
