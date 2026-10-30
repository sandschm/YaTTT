//! Aggregations for the statistics view and the timesheet. Day boundaries use the local time zone.

use crate::models::{Issue, Project, TimeEntry};
use crate::settings::WorkSettings;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: NaiveDate,
    pub seconds: i64,
    pub billable_seconds: i64,
    pub target_seconds: i64,
    pub break_seconds: i64,
    pub first_start: Option<DateTime<Utc>>,
    pub last_end: Option<DateTime<Utc>>,
    pub entries: usize,
    pub booked_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroupStat {
    pub key: String,
    pub label: String,
    pub seconds: i64,
    pub project_key: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub total_seconds: i64,
    pub billable_seconds: i64,
    pub target_seconds: i64,
    pub break_seconds: i64,
    pub booked_seconds: i64,
    pub unbooked_seconds: i64,
    pub work_days: usize,
    pub tracked_days: usize,
    pub avg_per_tracked_day_seconds: i64,
    pub days: Vec<DayStat>,
    pub by_project: Vec<GroupStat>,
    pub by_issue: Vec<GroupStat>,
}

/// A piece of an entry that lies within one local day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub date: NaiveDate,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

fn local_midnight_after(date: NaiveDate) -> DateTime<Utc> {
    let next = date + Duration::days(1);
    let naive = next.and_hms_opt(0, 0, 0).unwrap();
    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(a, _) => a.with_timezone(&Utc),
        chrono::LocalResult::None => Local
            .from_local_datetime(&(naive + Duration::hours(1)))
            .earliest()
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|| Utc.from_utc_datetime(&naive)),
    }
}

/// Split `[start, end)` into per-local-day segments.
pub fn split_by_local_day(start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cursor = start;
    let mut guard = 0;
    while cursor < end && guard < 400 {
        let date = cursor.with_timezone(&Local).date_naive();
        let boundary = local_midnight_after(date);
        let seg_end = if boundary < end { boundary } else { end };
        if seg_end > cursor {
            out.push(Segment {
                date,
                start: cursor,
                end: seg_end,
            });
        }
        cursor = seg_end.max(cursor + Duration::seconds(1));
        guard += 1;
    }
    out
}

pub fn target_seconds_for(date: NaiveDate, work: &WorkSettings) -> i64 {
    let idx = date.weekday().num_days_from_monday() as usize;
    (work.hours_per_weekday.get(idx).copied().unwrap_or(0.0) * 3600.0).round() as i64
}

/// Merge overlapping intervals and return the total gap between them (breaks).
type Interval = (DateTime<Utc>, DateTime<Utc>);

fn break_seconds(mut intervals: Vec<Interval>) -> i64 {
    if intervals.len() < 2 {
        return 0;
    }
    intervals.sort_by_key(|(s, _)| *s);
    let mut merged: Vec<Interval> = Vec::new();
    for (s, e) in intervals {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                if e > last.1 {
                    last.1 = e;
                }
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
        .windows(2)
        .map(|w| (w[1].0 - w[0].1).num_seconds().max(0))
        .sum()
}

pub fn compute(
    entries: &[TimeEntry],
    issues: &HashMap<String, Issue>,
    projects: &HashMap<String, Project>,
    from: NaiveDate,
    to: NaiveDate,
    work: &WorkSettings,
    now: DateTime<Utc>,
) -> Stats {
    let today = now.with_timezone(&Local).date_naive();
    let mut days: BTreeMap<NaiveDate, DayStat> = BTreeMap::new();
    let mut d = from;
    while d <= to {
        let target = if d <= today {
            target_seconds_for(d, work)
        } else {
            0
        };
        days.insert(
            d,
            DayStat {
                date: d,
                seconds: 0,
                billable_seconds: 0,
                target_seconds: target,
                break_seconds: 0,
                first_start: None,
                last_end: None,
                entries: 0,
                booked_seconds: 0,
            },
        );
        d += Duration::days(1);
    }
    let mut intervals_per_day: HashMap<NaiveDate, Vec<Interval>> = HashMap::new();
    let mut by_project: HashMap<String, i64> = HashMap::new();
    let mut by_issue: HashMap<String, i64> = HashMap::new();
    let mut booked_total = 0;

    for entry in entries {
        let end = entry.ended_at.unwrap_or(now);
        for seg in split_by_local_day(entry.started_at, end) {
            let Some(day) = days.get_mut(&seg.date) else {
                continue;
            };
            let secs = (seg.end - seg.start).num_seconds();
            day.seconds += secs;
            if entry.billable {
                day.billable_seconds += secs;
            }
            day.entries += 1;
            day.first_start = Some(day.first_start.map_or(seg.start, |f| f.min(seg.start)));
            day.last_end = Some(day.last_end.map_or(seg.end, |l| l.max(seg.end)));
            if entry.sync_state == crate::models::SyncState::Synced {
                day.booked_seconds += secs;
                booked_total += secs;
            }
            intervals_per_day
                .entry(seg.date)
                .or_default()
                .push((seg.start, seg.end));
            *by_issue.entry(entry.issue_key.clone()).or_insert(0) += secs;
            let project_key = issues
                .get(&entry.issue_key)
                .map(|i| i.project_key.clone())
                .unwrap_or_else(|| {
                    entry
                        .issue_key
                        .rsplit_once('-')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default()
                });
            *by_project.entry(project_key).or_insert(0) += secs;
        }
    }
    for (date, intervals) in intervals_per_day {
        if let Some(day) = days.get_mut(&date) {
            day.break_seconds = break_seconds(intervals);
        }
    }

    let days: Vec<DayStat> = days.into_values().collect();
    let total_seconds: i64 = days.iter().map(|d| d.seconds).sum();
    let billable_seconds: i64 = days.iter().map(|d| d.billable_seconds).sum();
    let target_seconds: i64 = days.iter().map(|d| d.target_seconds).sum();
    let break_total: i64 = days.iter().map(|d| d.break_seconds).sum();
    let work_days = days.iter().filter(|d| d.target_seconds > 0).count();
    let tracked_days = days.iter().filter(|d| d.seconds > 0).count();

    let mut by_project: Vec<GroupStat> = by_project
        .into_iter()
        .map(|(key, seconds)| {
            let p = projects.get(&key);
            GroupStat {
                label: p.map(|p| p.name.clone()).unwrap_or_else(|| key.clone()),
                color: p.and_then(|p| p.color.clone()),
                project_key: Some(key.clone()),
                key,
                seconds,
            }
        })
        .collect();
    by_project.sort_by_key(|g| std::cmp::Reverse(g.seconds));
    let mut by_issue: Vec<GroupStat> = by_issue
        .into_iter()
        .map(|(key, seconds)| {
            let i = issues.get(&key);
            let project_key = i.map(|i| i.project_key.clone());
            GroupStat {
                label: i.map(|i| i.summary.clone()).unwrap_or_default(),
                color: project_key
                    .as_ref()
                    .and_then(|p| projects.get(p))
                    .and_then(|p| p.color.clone()),
                project_key,
                key,
                seconds,
            }
        })
        .collect();
    by_issue.sort_by_key(|g| std::cmp::Reverse(g.seconds));

    Stats {
        from,
        to,
        total_seconds,
        billable_seconds,
        target_seconds,
        break_seconds: break_total,
        booked_seconds: booked_total,
        unbooked_seconds: total_seconds - booked_total,
        work_days,
        tracked_days,
        avg_per_tracked_day_seconds: if tracked_days > 0 {
            total_seconds / tracked_days as i64
        } else {
            0
        },
        days,
        by_project,
        by_issue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SyncState;

    fn entry(
        id: i64,
        key: &str,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        synced: bool,
    ) -> TimeEntry {
        TimeEntry {
            id,
            issue_key: key.into(),
            started_at: start,
            ended_at: end,
            note: String::new(),
            billable: true,
            sync_state: if synced {
                SyncState::Synced
            } else {
                SyncState::Local
            },
            jira_worklog_id: None,
            tempo_worklog_id: None,
            sync_error: None,
            booked_seconds: None,
            created_at: start,
            updated_at: start,
        }
    }

    #[test]
    fn splits_entries_crossing_midnight() {
        // Use local times so the test is independent of the machine's time zone.
        let day = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let start = Local
            .from_local_datetime(&day.and_hms_opt(23, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let end = start + Duration::hours(2);
        let segs = split_by_local_day(start, end);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].date, day);
        assert_eq!(segs[1].date, day + Duration::days(1));
        assert_eq!((segs[0].end - segs[0].start).num_seconds(), 3600);
        assert_eq!((segs[1].end - segs[1].start).num_seconds(), 3600);
    }

    #[test]
    fn totals_breaks_and_targets() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(); // Wednesday
        let at = |h: u32, m: u32| {
            Local
                .from_local_datetime(&day.and_hms_opt(h, m, 0).unwrap())
                .unwrap()
                .with_timezone(&Utc)
        };
        let entries = vec![
            entry(1, "A-1", at(9, 0), Some(at(11, 0)), true),
            entry(2, "A-2", at(11, 30), Some(at(12, 30)), false),
            entry(3, "B-1", at(12, 0), Some(at(13, 0)), false), // overlaps entry 2
        ];
        let mut issues = HashMap::new();
        for (k, p) in [("A-1", "A"), ("A-2", "A"), ("B-1", "B")] {
            issues.insert(
                k.to_string(),
                Issue {
                    id: "1".into(),
                    key: k.into(),
                    project_key: p.into(),
                    summary: k.into(),
                    issue_type: "Task".into(),
                    issue_type_id: None,
                    issue_type_icon: None,
                    is_subtask: false,
                    status: "Open".into(),
                    status_category: "new".into(),
                    parent_key: None,
                    epic_key: None,
                    sprint_id: None,
                    sprint_name: None,
                    assignee: None,
                    assignee_id: None,
                    priority: None,
                    due_date: None,
                    original_estimate_s: None,
                    remaining_estimate_s: None,
                    time_spent_s: None,
                    updated: None,
                    favorite: false,
                    source: "sprint".into(),
                    last_seen: None,
                },
            );
        }
        let work = WorkSettings::default();
        let stats = compute(
            &entries,
            &issues,
            &HashMap::new(),
            day,
            day,
            &work,
            at(18, 0),
        );
        assert_eq!(stats.total_seconds, 4 * 3600);
        assert_eq!(stats.days[0].break_seconds, 1800); // 11:00 -> 11:30
        assert_eq!(stats.days[0].target_seconds, 8 * 3600);
        assert_eq!(stats.booked_seconds, 7200);
        assert_eq!(stats.unbooked_seconds, 7200);
        assert_eq!(stats.by_project[0].key, "A");
        assert_eq!(stats.by_project[0].seconds, 3 * 3600);
        assert_eq!(stats.work_days, 1);
        assert_eq!(stats.avg_per_tracked_day_seconds, 4 * 3600);
    }

    #[test]
    fn future_days_have_no_target() {
        let today = Local::now().date_naive();
        let tomorrow = today + Duration::days(1);
        let work = WorkSettings {
            hours_per_weekday: [8.0; 7],
            ..Default::default()
        };
        let stats = compute(
            &[],
            &HashMap::new(),
            &HashMap::new(),
            today,
            tomorrow,
            &work,
            Utc::now(),
        );
        assert_eq!(stats.days[0].target_seconds, 8 * 3600);
        assert_eq!(stats.days[1].target_seconds, 0);
    }
}
