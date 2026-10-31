//! CSV / JSON export of time entries.

use crate::error::Result;
use crate::i18n::Lang;
use crate::models::{Issue, TimeEntry};
use crate::settings::TimeFormat;
use crate::{format_duration_in, stats::split_by_local_day};
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    pub date: NaiveDate,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub duration_seconds: i64,
    pub project_key: String,
    pub issue_key: String,
    pub summary: String,
    pub note: String,
    pub billable: bool,
    pub sync_state: String,
    pub jira_worklog_id: Option<String>,
    pub entry_ids: Vec<i64>,
}

/// One row per entry segment (entries crossing midnight are split).
pub fn build_rows(
    entries: &[TimeEntry],
    issues: &HashMap<String, Issue>,
    now: DateTime<Utc>,
) -> Vec<ExportRow> {
    let mut rows = Vec::new();
    for e in entries {
        let end = e.ended_at.unwrap_or(now);
        let issue = issues.get(&e.issue_key);
        for seg in split_by_local_day(e.started_at, end) {
            rows.push(ExportRow {
                date: seg.date,
                start: seg.start,
                end: seg.end,
                duration_seconds: (seg.end - seg.start).num_seconds(),
                project_key: issue.map(|i| i.project_key.clone()).unwrap_or_else(|| {
                    e.issue_key
                        .rsplit_once('-')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default()
                }),
                issue_key: e.issue_key.clone(),
                summary: issue.map(|i| i.summary.clone()).unwrap_or_default(),
                note: e.note.clone(),
                billable: e.billable,
                sync_state: e.sync_state.as_str().to_string(),
                jira_worklog_id: e.jira_worklog_id.clone(),
                entry_ids: vec![e.id],
            });
        }
    }
    rows.sort_by_key(|r| r.start);
    rows
}

/// One row per day and issue.
pub fn summarize_rows(rows: &[ExportRow]) -> Vec<ExportRow> {
    let mut groups: BTreeMap<(NaiveDate, String), ExportRow> = BTreeMap::new();
    for r in rows {
        let key = (r.date, r.issue_key.clone());
        match groups.get_mut(&key) {
            Some(g) => {
                g.duration_seconds += r.duration_seconds;
                g.start = g.start.min(r.start);
                g.end = g.end.max(r.end);
                if !r.note.is_empty() && !g.note.contains(&r.note) {
                    if !g.note.is_empty() {
                        g.note.push_str(" | ");
                    }
                    g.note.push_str(&r.note);
                }
                g.billable = g.billable && r.billable;
                if g.sync_state != r.sync_state {
                    g.sync_state = "mixed".into();
                }
                g.jira_worklog_id = None;
                g.entry_ids.extend(r.entry_ids.iter().copied());
            }
            None => {
                groups.insert(key, r.clone());
            }
        }
    }
    groups.into_values().collect()
}

pub fn to_csv(rows: &[ExportRow], time_format: TimeFormat) -> Result<String> {
    to_csv_in(rows, time_format, crate::i18n::lang())
}

/// CSV in the conventions of `lang`: German uses `;`, `dd.mm.yyyy` and a decimal comma (what
/// Excel expects in a German locale), English uses `,`, ISO dates and a decimal point.
pub fn to_csv_in(rows: &[ExportRow], time_format: TimeFormat, lang: Lang) -> Result<String> {
    let (delimiter, date_format, yes, no, header) = match lang {
        Lang::De => (
            b';',
            "%d.%m.%Y",
            "ja",
            "nein",
            [
                "Datum",
                "Start",
                "Ende",
                "Dauer",
                "Dauer (Sekunden)",
                "Projekt",
                "Ticket",
                "Titel",
                "Notiz",
                "Abrechenbar",
                "Buchungsstatus",
                "Jira-Worklog-ID",
            ],
        ),
        Lang::En => (
            b',',
            "%Y-%m-%d",
            "yes",
            "no",
            [
                "Date",
                "Start",
                "End",
                "Duration",
                "Duration (seconds)",
                "Project",
                "Ticket",
                "Title",
                "Note",
                "Billable",
                "Booking status",
                "Jira worklog ID",
            ],
        ),
    };
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(vec![]);
    wtr.write_record(header)?;
    for r in rows {
        wtr.write_record([
            r.date.format(date_format).to_string(),
            r.start.with_timezone(&Local).format("%H:%M").to_string(),
            r.end.with_timezone(&Local).format("%H:%M").to_string(),
            format_duration_in(r.duration_seconds, time_format, lang),
            r.duration_seconds.to_string(),
            r.project_key.clone(),
            r.issue_key.clone(),
            r.summary.clone(),
            r.note.clone(),
            if r.billable { yes.into() } else { no.into() },
            r.sync_state.clone(),
            r.jira_worklog_id.clone().unwrap_or_default(),
        ])?;
    }
    let bytes = wtr
        .into_inner()
        .map_err(|e| crate::error::Error::Io(e.to_string()))?;
    // BOM so that Excel recognises UTF-8.
    Ok(format!("\u{feff}{}", String::from_utf8_lossy(&bytes)))
}

pub fn to_json(rows: &[ExportRow]) -> Result<String> {
    Ok(serde_json::to_string_pretty(rows)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SyncState;
    use chrono::TimeZone;

    fn entry(
        id: i64,
        key: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        note: &str,
    ) -> TimeEntry {
        TimeEntry {
            id,
            issue_key: key.into(),
            started_at: start,
            ended_at: Some(end),
            note: note.into(),
            billable: true,
            sync_state: SyncState::Local,
            jira_worklog_id: None,
            tempo_worklog_id: None,
            sync_error: None,
            booked_seconds: None,
            created_at: start,
            updated_at: start,
        }
    }

    #[test]
    fn csv_and_summary() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let at = |h: u32, m: u32| {
            Local
                .from_local_datetime(&day.and_hms_opt(h, m, 0).unwrap())
                .unwrap()
                .with_timezone(&Utc)
        };
        let entries = vec![
            entry(1, "A-1", at(9, 0), at(10, 0), "x"),
            entry(2, "A-1", at(13, 0), at(13, 30), "y"),
            entry(3, "B-2", at(10, 0), at(11, 0), ""),
        ];
        let rows = build_rows(&entries, &HashMap::new(), at(18, 0));
        assert_eq!(rows.len(), 3);
        let csv = to_csv_in(&rows, TimeFormat::HoursMinutes, Lang::De).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.starts_with("\u{feff}Datum;Start;Ende;"));
        assert!(csv.contains("28.10.2026;09:00;10:00;1:00;3600;A;A-1;"));
        assert!(csv.contains(";ja;"));
        let csv = to_csv_in(&rows, TimeFormat::Decimal, Lang::En).unwrap();
        assert!(csv.starts_with("\u{feff}Date,Start,End,"));
        assert!(csv.contains("2026-10-28,09:00,10:00,1.00,3600,A,A-1,"));
        assert!(csv.contains(",yes,"));
        let summary = summarize_rows(&rows);
        assert_eq!(summary.len(), 2);
        let a = summary.iter().find(|r| r.issue_key == "A-1").unwrap();
        assert_eq!(a.duration_seconds, 5400);
        assert_eq!(a.note, "x | y");
        assert_eq!(a.entry_ids, vec![1, 2]);
        let json = to_json(&summary).unwrap();
        assert!(json.contains("\"issueKey\": \"B-2\""));
    }
}
