//! Booking of time entries to Jira / Tempo and import of existing worklogs.

use crate::error::{Error, Result};
use crate::jira::{parse_jira_timestamp, JiraClient};
use crate::models::*;
use crate::rounding::round_seconds;
use crate::settings::{Rounding, TempoMode};
use crate::storage::Store;
use crate::tempo::{TempoCloud, TempoDataCenter};
use chrono::{DateTime, Duration, NaiveDate, Utc};

/// Everything needed to talk to the booking target.
pub struct BookingContext<'a> {
    pub jira: &'a JiraClient,
    pub tempo_cloud: Option<&'a TempoCloud>,
    pub mode: TempoMode,
    pub user: &'a JiraUser,
    pub rounding: Rounding,
}

impl<'a> BookingContext<'a> {
    fn effective_mode(&self) -> TempoMode {
        match self.mode {
            TempoMode::TempoCloud if self.tempo_cloud.is_none() => TempoMode::JiraWorklog,
            m => m,
        }
    }

    fn worker_key(&self) -> &str {
        self.user.key.as_deref().unwrap_or(&self.user.id)
    }
}

fn issue_numeric_id(store: &Store, issue_key: &str) -> Result<i64> {
    let issue = store.get_issue(issue_key)?.ok_or_else(|| {
        Error::NotFound(crate::tr!(
            "Ticket {issue_key} ist lokal nicht bekannt",
            "Ticket {issue_key} is not known locally",
            issue_key = issue_key
        ))
    })?;
    issue.id.parse::<i64>().map_err(|_| {
        Error::Parse(crate::tr!(
            "Ticket-ID {id} ist nicht numerisch",
            "Ticket ID {id} is not numeric",
            id = issue.id
        ))
    })
}

/// Book (create or update) a single stopped entry.
pub async fn book_entry(
    ctx: &BookingContext<'_>,
    store: &Store,
    entry_id: i64,
    now: DateTime<Utc>,
) -> Result<BookingResult> {
    let entry = store.get_entry(entry_id)?.ok_or_else(|| {
        Error::NotFound(crate::tr!(
            "Eintrag {entry_id}",
            "Entry {entry_id}",
            entry_id = entry_id
        ))
    })?;
    if entry.is_running() {
        return Err(Error::Invalid(
            crate::tr!(
                "Laufende Timer können nicht gebucht werden",
                "Running timers cannot be booked"
            )
            .into(),
        ));
    }
    let raw_seconds = entry.duration_seconds(now);
    let seconds = round_seconds(raw_seconds, ctx.rounding).max(60);
    let billable = if entry.billable { seconds } else { 0 };
    let comment = entry.note.trim();

    let result = match ctx.effective_mode() {
        TempoMode::JiraWorklog => book_via_jira(ctx, &entry, seconds, comment).await,
        TempoMode::TempoDataCenter => {
            book_via_tempo_dc(ctx, &entry, seconds, billable, comment).await
        }
        TempoMode::TempoCloud => {
            book_via_tempo_cloud(ctx, store, &entry, seconds, billable, comment).await
        }
    };

    match result {
        Ok((jira_id, tempo_id)) => {
            store.set_sync_result(
                entry.id,
                SyncState::Synced,
                jira_id.as_deref(),
                tempo_id.as_deref(),
                Some(seconds),
                None,
                now,
            )?;
            Ok(BookingResult {
                entry_id: entry.id,
                jira_worklog_id: jira_id,
                tempo_worklog_id: tempo_id,
                booked_seconds: seconds,
            })
        }
        Err(e) => {
            let msg = e.to_string();
            store.set_sync_state(entry.id, SyncState::Failed, Some(&msg), now)?;
            Err(e)
        }
    }
}

async fn book_via_jira(
    ctx: &BookingContext<'_>,
    entry: &TimeEntry,
    seconds: i64,
    comment: &str,
) -> Result<(Option<String>, Option<String>)> {
    if let Some(id) = &entry.jira_worklog_id {
        match ctx
            .jira
            .update_worklog(&entry.issue_key, id, &entry.started_at, seconds, comment)
            .await
        {
            Ok(()) => return Ok((Some(id.clone()), entry.tempo_worklog_id.clone())),
            Err(Error::Http { status: 404, .. }) => { /* remote gone, create again */ }
            Err(e) => return Err(e),
        }
    }
    let id = ctx
        .jira
        .add_worklog(&entry.issue_key, &entry.started_at, seconds, comment)
        .await?;
    Ok((Some(id), None))
}

async fn book_via_tempo_dc(
    ctx: &BookingContext<'_>,
    entry: &TimeEntry,
    seconds: i64,
    billable: i64,
    comment: &str,
) -> Result<(Option<String>, Option<String>)> {
    let tempo = TempoDataCenter { jira: ctx.jira };
    if let Some(id) = &entry.tempo_worklog_id {
        match tempo
            .update_worklog(
                id,
                &entry.issue_key,
                ctx.worker_key(),
                &entry.started_at,
                seconds,
                billable,
                comment,
            )
            .await
        {
            Ok(()) => return Ok((entry.jira_worklog_id.clone(), Some(id.clone()))),
            Err(Error::Http { status: 404, .. }) => {}
            Err(e) => return Err(e),
        }
    }
    let ids = tempo
        .create_worklog(
            &entry.issue_key,
            ctx.worker_key(),
            &entry.started_at,
            seconds,
            billable,
            comment,
        )
        .await?;
    Ok((ids.jira_worklog_id, Some(ids.tempo_worklog_id)))
}

async fn book_via_tempo_cloud(
    ctx: &BookingContext<'_>,
    store: &Store,
    entry: &TimeEntry,
    seconds: i64,
    billable: i64,
    comment: &str,
) -> Result<(Option<String>, Option<String>)> {
    let tempo = ctx.tempo_cloud.ok_or_else(|| {
        Error::Invalid(crate::tr!("Tempo-Cloud-Token fehlt", "Tempo Cloud token is missing").into())
    })?;
    let issue_id = issue_numeric_id(store, &entry.issue_key)?;
    if let Some(id) = &entry.tempo_worklog_id {
        match tempo
            .update_worklog(
                id,
                issue_id,
                &ctx.user.id,
                &entry.started_at,
                seconds,
                billable,
                comment,
            )
            .await
        {
            Ok(()) => return Ok((entry.jira_worklog_id.clone(), Some(id.clone()))),
            Err(Error::Http { status: 404, .. }) => {}
            Err(e) => return Err(e),
        }
    }
    let ids = tempo
        .create_worklog(
            issue_id,
            &ctx.user.id,
            &entry.started_at,
            seconds,
            billable,
            comment,
        )
        .await?;
    Ok((ids.jira_worklog_id, Some(ids.tempo_worklog_id)))
}

/// Delete the remote worklog(s) of a locally deleted entry, then purge it.
pub async fn unbook_entry(
    ctx: &BookingContext<'_>,
    store: &Store,
    entry: &TimeEntry,
) -> Result<()> {
    if let Some(id) = &entry.tempo_worklog_id {
        match ctx.effective_mode() {
            TempoMode::TempoCloud => {
                if let Some(tempo) = ctx.tempo_cloud {
                    tempo.delete_worklog(id).await?;
                }
            }
            _ => {
                TempoDataCenter { jira: ctx.jira }
                    .delete_worklog(id)
                    .await?
            }
        }
    } else if let Some(id) = &entry.jira_worklog_id {
        ctx.jira.delete_worklog(&entry.issue_key, id).await?;
    }
    store.purge_deleted_entry(entry.id)?;
    Ok(())
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub booked: Vec<i64>,
    pub failed: Vec<(i64, String)>,
    pub deleted_remote: usize,
}

/// Book all pending/modified entries and clean up deleted ones.
pub async fn process_pending(
    ctx: &BookingContext<'_>,
    store: &Store,
    now: DateTime<Utc>,
) -> Result<SyncSummary> {
    let mut summary = SyncSummary::default();
    let mut todo = store.entries_with_state(SyncState::Pending)?;
    todo.extend(store.entries_with_state(SyncState::Modified)?);
    for entry in todo {
        match book_entry(ctx, store, entry.id, now).await {
            Ok(_) => summary.booked.push(entry.id),
            Err(Error::AuthExpired(code)) => return Err(Error::AuthExpired(code)),
            Err(e) => summary.failed.push((entry.id, e.to_string())),
        }
    }
    for entry in store.deleted_entries_with_remote()? {
        match unbook_entry(ctx, store, &entry).await {
            Ok(()) => summary.deleted_remote += 1,
            Err(Error::AuthExpired(code)) => return Err(Error::AuthExpired(code)),
            Err(e) => tracing::warn!("remote delete of entry {} failed: {e}", entry.id),
        }
    }
    Ok(summary)
}

/// Import the user's existing Jira worklogs for a date range as synced entries.
/// Returns the number of newly created entries.
pub async fn import_worklogs(
    ctx: &BookingContext<'_>,
    store: &Store,
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
) -> Result<usize> {
    let logs = ctx.jira.my_worklogs(ctx.user, from, to).await?;
    let mut created = 0;
    for log in logs {
        if store.entry_by_jira_worklog(&log.id)?.is_some() {
            continue;
        }
        let Some(started) = parse_jira_timestamp(&log.started) else {
            continue;
        };
        if store.get_issue(&log.issue_key)?.is_none() {
            if let Ok(mut issue) = ctx.jira.get_issue(&log.issue_key).await {
                issue.source = "tracked".into();
                store.upsert_issues(&[issue], now)?;
            }
        }
        let input = TimeEntryInput {
            issue_key: log.issue_key.clone(),
            started_at: started,
            ended_at: Some(started + Duration::seconds(log.time_spent_seconds)),
            note: log.comment.clone(),
            billable: true,
        };
        store.insert_imported_entry(&input, Some(&log.id), None, log.time_spent_seconds, now)?;
        created += 1;
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::RoundingMode;
    use chrono::TimeZone;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, h, m, 0).unwrap()
    }

    fn user() -> JiraUser {
        JiraUser {
            id: "sam".into(),
            key: Some("JIRAUSER1".into()),
            display_name: "Sam".into(),
            email: None,
            avatar_url: None,
            time_zone: None,
        }
    }

    #[tokio::test]
    async fn books_with_rounding_and_marks_synced() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .and(body_partial_json(
                json!({"timeSpentSeconds": 1800, "comment": "work"}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "99"})))
            .mount(&server)
            .await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        let entry = store
            .insert_entry(
                &TimeEntryInput {
                    issue_key: "A-1".into(),
                    started_at: t(9, 0),
                    ended_at: Some(t(9, 25)),
                    note: "work".into(),
                    billable: true,
                },
                t(9, 25),
            )
            .unwrap();
        let u = user();
        let ctx = BookingContext {
            jira: &jira,
            tempo_cloud: None,
            mode: TempoMode::JiraWorklog,
            user: &u,
            rounding: Rounding {
                minutes: 15,
                mode: RoundingMode::Up,
            },
        };
        let res = book_entry(&ctx, &store, entry.id, t(9, 30)).await.unwrap();
        assert_eq!(res.booked_seconds, 1800);
        let stored = store.get_entry(entry.id).unwrap().unwrap();
        assert_eq!(stored.sync_state, SyncState::Synced);
        assert_eq!(stored.jira_worklog_id.as_deref(), Some("99"));
    }

    #[tokio::test]
    async fn failure_is_recorded() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/2/issue/A-1/worklog"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"errorMessages": ["nope"]})),
            )
            .mount(&server)
            .await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        let entry = store
            .insert_entry(
                &TimeEntryInput {
                    issue_key: "A-1".into(),
                    started_at: t(9, 0),
                    ended_at: Some(t(9, 25)),
                    note: "".into(),
                    billable: true,
                },
                t(9, 25),
            )
            .unwrap();
        store
            .set_sync_state(entry.id, SyncState::Pending, None, t(9, 25))
            .unwrap();
        let u = user();
        let ctx = BookingContext {
            jira: &jira,
            tempo_cloud: None,
            mode: TempoMode::JiraWorklog,
            user: &u,
            rounding: Rounding::default(),
        };
        let summary = process_pending(&ctx, &store, t(9, 30)).await.unwrap();
        assert_eq!(summary.failed.len(), 1);
        let stored = store.get_entry(entry.id).unwrap().unwrap();
        assert_eq!(stored.sync_state, SyncState::Failed);
        assert!(stored.sync_error.unwrap().contains("nope"));
    }

    #[tokio::test]
    async fn modified_entry_is_updated_not_recreated() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/2/issue/A-1/worklog/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "5"})))
            .expect(1)
            .mount(&server)
            .await;
        let jira = JiraClient::new(
            &server.uri(),
            Credential::Bearer { token: "t".into() },
            Deployment::DataCenter,
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        let entry = store
            .insert_entry(
                &TimeEntryInput {
                    issue_key: "A-1".into(),
                    started_at: t(9, 0),
                    ended_at: Some(t(10, 0)),
                    note: "".into(),
                    billable: true,
                },
                t(10, 0),
            )
            .unwrap();
        store
            .set_sync_result(
                entry.id,
                SyncState::Synced,
                Some("5"),
                None,
                Some(3600),
                None,
                t(10, 0),
            )
            .unwrap();
        store
            .update_entry(
                entry.id,
                &TimeEntryInput {
                    issue_key: "A-1".into(),
                    started_at: t(9, 0),
                    ended_at: Some(t(10, 30)),
                    note: "".into(),
                    billable: true,
                },
                t(10, 30),
            )
            .unwrap();
        let u = user();
        let ctx = BookingContext {
            jira: &jira,
            tempo_cloud: None,
            mode: TempoMode::JiraWorklog,
            user: &u,
            rounding: Rounding::default(),
        };
        let summary = process_pending(&ctx, &store, t(10, 31)).await.unwrap();
        assert_eq!(summary.booked, vec![entry.id]);
        assert_eq!(
            store.get_entry(entry.id).unwrap().unwrap().booked_seconds,
            Some(5400)
        );
    }
}
