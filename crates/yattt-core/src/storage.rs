//! SQLite persistence. All timestamps are stored as RFC 3339 strings in UTC, which sort
//! lexicographically and are readable when inspecting the database by hand.

use crate::error::{Error, Result};
use crate::models::*;
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 3;

pub struct Store {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

fn ts(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            Error::Parse(crate::tr!(
                "Zeitstempel '{s}': {e}",
                "Timestamp '{s}': {e}",
                s = s,
                e = e
            ))
        })
}

/// Labels and components are stored as JSON arrays so that any character Jira allows survives the round trip.
fn labels_to_db(labels: &[String]) -> String {
    if labels.is_empty() {
        String::new()
    } else {
        serde_json::to_string(labels).unwrap_or_default()
    }
}

fn labels_from_db(s: &str) -> Vec<String> {
    if s.is_empty() {
        vec![]
    } else {
        serde_json::from_str(s).unwrap_or_default()
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        };
        store.init()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
            path: None,
        };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS projects (
                key TEXT PRIMARY KEY,
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                avatar_url TEXT,
                color TEXT,
                selected INTEGER NOT NULL DEFAULT 0,
                sort INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS sprints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                state TEXT NOT NULL,
                board_id INTEGER,
                start_date TEXT,
                end_date TEXT,
                goal TEXT,
                selected INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS issues (
                key TEXT PRIMARY KEY,
                id TEXT NOT NULL,
                project_key TEXT NOT NULL,
                summary TEXT NOT NULL,
                issue_type TEXT NOT NULL,
                issue_type_id TEXT,
                issue_type_icon TEXT,
                is_subtask INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                status_category TEXT NOT NULL,
                parent_key TEXT,
                epic_key TEXT,
                sprint_id INTEGER,
                sprint_name TEXT,
                assignee TEXT,
                assignee_id TEXT,
                priority TEXT,
                due_date TEXT,
                original_estimate_s INTEGER,
                remaining_estimate_s INTEGER,
                time_spent_s INTEGER,
                updated TEXT,
                favorite INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'sprint',
                last_seen TEXT,
                labels TEXT NOT NULL DEFAULT '',
                components TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_key);
            CREATE TABLE IF NOT EXISTS time_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                issue_key TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                note TEXT NOT NULL DEFAULT '',
                billable INTEGER NOT NULL DEFAULT 1,
                sync_state TEXT NOT NULL DEFAULT 'local',
                jira_worklog_id TEXT,
                tempo_worklog_id TEXT,
                sync_error TEXT,
                booked_seconds INTEGER,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_entries_started ON time_entries(started_at);
            CREATE INDEX IF NOT EXISTS idx_entries_issue ON time_entries(issue_key);
            CREATE INDEX IF NOT EXISTS idx_entries_running ON time_entries(ended_at) WHERE ended_at IS NULL;",
        )?;
        // Schema version 2 (0.2.0) added `labels`, version 3 (0.3.0) `components`. Older
        // databases get the missing columns added in place.
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(issues)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for column in ["labels", "components"] {
            if !columns.iter().any(|c| c == column) {
                conn.execute_batch(&format!(
                    "ALTER TABLE issues ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                ))?;
            }
        }
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- meta

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_meta(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    // ------------------------------------------------------------ projects

    pub fn upsert_projects(&self, projects: &[Project]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for p in projects {
            tx.execute(
                "INSERT INTO projects(key, id, name, avatar_url, color, selected)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(key) DO UPDATE SET id = excluded.id, name = excluded.name,
                    avatar_url = excluded.avatar_url,
                    color = COALESCE(projects.color, excluded.color),
                    selected = excluded.selected",
                params![
                    p.key,
                    p.id,
                    p.name,
                    p.avatar_url,
                    p.color,
                    p.selected as i32
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, id, name, avatar_url, color, selected FROM projects ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Project {
                key: r.get(0)?,
                id: r.get(1)?,
                name: r.get(2)?,
                avatar_url: r.get(3)?,
                color: r.get(4)?,
                selected: r.get::<_, i32>(5)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_project_color(&self, key: &str, color: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE projects SET color = ?2 WHERE key = ?1",
            params![key, color],
        )?;
        Ok(())
    }

    pub fn set_selected_projects(&self, keys: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("UPDATE projects SET selected = 0", [])?;
        for k in keys {
            tx.execute(
                "UPDATE projects SET selected = 1 WHERE key = ?1",
                params![k],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------- sprints

    pub fn upsert_sprints(&self, sprints: &[Sprint]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for s in sprints {
            tx.execute(
                "INSERT INTO sprints(id, name, state, board_id, start_date, end_date, goal, selected)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = excluded.state,
                    board_id = COALESCE(excluded.board_id, sprints.board_id),
                    start_date = excluded.start_date, end_date = excluded.end_date, goal = excluded.goal,
                    selected = excluded.selected",
                params![s.id, s.name, s.state, s.board_id, s.start_date, s.end_date, s.goal, s.selected as i32],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_sprints(&self) -> Result<Vec<Sprint>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, state, board_id, start_date, end_date, goal, selected FROM sprints ORDER BY state, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Sprint {
                id: r.get(0)?,
                name: r.get(1)?,
                state: r.get(2)?,
                board_id: r.get(3)?,
                start_date: r.get(4)?,
                end_date: r.get(5)?,
                goal: r.get(6)?,
                selected: r.get::<_, i32>(7)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_selected_sprints(&self, ids: &[i64]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("UPDATE sprints SET selected = 0", [])?;
        for id in ids {
            tx.execute("UPDATE sprints SET selected = 1 WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(())
    }

    // -------------------------------------------------------------- issues

    fn issue_from_row(r: &Row<'_>) -> rusqlite::Result<Issue> {
        let last_seen: Option<String> = r.get(24)?;
        let labels: String = r.get(25)?;
        let components: String = r.get(26)?;
        Ok(Issue {
            key: r.get(0)?,
            id: r.get(1)?,
            project_key: r.get(2)?,
            summary: r.get(3)?,
            issue_type: r.get(4)?,
            issue_type_id: r.get(5)?,
            issue_type_icon: r.get(6)?,
            is_subtask: r.get::<_, i32>(7)? != 0,
            status: r.get(8)?,
            status_category: r.get(9)?,
            parent_key: r.get(10)?,
            epic_key: r.get(11)?,
            sprint_id: r.get(12)?,
            sprint_name: r.get(13)?,
            assignee: r.get(14)?,
            assignee_id: r.get(15)?,
            priority: r.get(16)?,
            due_date: r.get(17)?,
            original_estimate_s: r.get(18)?,
            remaining_estimate_s: r.get(19)?,
            time_spent_s: r.get(20)?,
            updated: r.get(21)?,
            labels: labels_from_db(&labels),
            components: labels_from_db(&components),
            favorite: r.get::<_, i32>(22)? != 0,
            source: r.get(23)?,
            last_seen: last_seen.and_then(|s| parse_ts(&s).ok()),
        })
    }

    const ISSUE_COLUMNS: &'static str = "key, id, project_key, summary, issue_type, issue_type_id, issue_type_icon, is_subtask, status, status_category, parent_key, epic_key, sprint_id, sprint_name, assignee, assignee_id, priority, due_date, original_estimate_s, remaining_estimate_s, time_spent_s, updated, favorite, source, last_seen, labels, components";

    /// Insert or update issues. The local `favorite` flag is preserved; `source` is only
    /// upgraded (a tracked issue that appears in a sprint becomes a sprint issue, never the reverse
    /// unless `force_source` is set).
    pub fn upsert_issues(&self, issues: &[Issue], now: DateTime<Utc>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for i in issues {
            tx.execute(
                "INSERT INTO issues(key, id, project_key, summary, issue_type, issue_type_id, issue_type_icon, is_subtask,
                    status, status_category, parent_key, epic_key, sprint_id, sprint_name, assignee, assignee_id, priority,
                    due_date, original_estimate_s, remaining_estimate_s, time_spent_s, updated, favorite, source, last_seen, labels, components)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
                 ON CONFLICT(key) DO UPDATE SET id = excluded.id, project_key = excluded.project_key,
                    summary = excluded.summary, issue_type = excluded.issue_type, issue_type_id = excluded.issue_type_id,
                    issue_type_icon = excluded.issue_type_icon, is_subtask = excluded.is_subtask, status = excluded.status,
                    status_category = excluded.status_category, parent_key = excluded.parent_key, epic_key = excluded.epic_key,
                    sprint_id = excluded.sprint_id, sprint_name = excluded.sprint_name, assignee = excluded.assignee,
                    assignee_id = excluded.assignee_id, priority = excluded.priority, due_date = excluded.due_date,
                    original_estimate_s = excluded.original_estimate_s, remaining_estimate_s = excluded.remaining_estimate_s,
                    time_spent_s = excluded.time_spent_s, updated = excluded.updated, labels = excluded.labels, components = excluded.components,
                    source = CASE WHEN excluded.source IN ('sprint','selected-sprint') THEN excluded.source ELSE issues.source END,
                    last_seen = excluded.last_seen",
                params![
                    i.key, i.id, i.project_key, i.summary, i.issue_type, i.issue_type_id, i.issue_type_icon,
                    i.is_subtask as i32, i.status, i.status_category, i.parent_key, i.epic_key, i.sprint_id,
                    i.sprint_name, i.assignee, i.assignee_id, i.priority, i.due_date, i.original_estimate_s,
                    i.remaining_estimate_s, i.time_spent_s, i.updated, i.favorite as i32, i.source, ts(&now),
                    labels_to_db(&i.labels), labels_to_db(&i.components)
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_issues(&self) -> Result<Vec<Issue>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM issues ORDER BY project_key, key",
            Self::ISSUE_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::issue_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_issue(&self, key: &str) -> Result<Option<Issue>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM issues WHERE key = ?1", Self::ISSUE_COLUMNS);
        Ok(conn
            .query_row(&sql, params![key], Self::issue_from_row)
            .optional()?)
    }

    /// Marks or unmarks a favorite. Returns `false` (and changes nothing) when marking would
    /// exceed [`MAX_FAVORITES`].
    pub fn set_favorite(&self, key: &str, favorite: bool) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if favorite {
            let already: i64 = tx.query_row(
                "SELECT COUNT(*) FROM issues WHERE key = ?1 AND favorite = 1",
                params![key],
                |r| r.get(0),
            )?;
            if already == 0 {
                let count: i64 =
                    tx.query_row("SELECT COUNT(*) FROM issues WHERE favorite = 1", [], |r| {
                        r.get(0)
                    })?;
                if count as usize >= MAX_FAVORITES {
                    return Ok(false);
                }
            }
        }
        tx.execute(
            "UPDATE issues SET favorite = ?2 WHERE key = ?1",
            params![key, favorite as i32],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn favorite_count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM issues WHERE favorite = 1", [], |r| {
            r.get(0)
        })?;
        Ok(n as usize)
    }

    /// Favorites ordered by project and key, as shown in the tray menu.
    pub fn favorite_issues(&self) -> Result<Vec<Issue>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM issues WHERE favorite = 1 ORDER BY project_key, key",
            Self::ISSUE_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::issue_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Remove sprint-sourced issues that were not part of the latest refresh and have neither
    /// time entries nor a favorite flag. Returns the number of removed rows.
    pub fn prune_stale_issues(&self, seen_before: DateTime<Utc>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "DELETE FROM issues WHERE source IN ('sprint','selected-sprint') AND favorite = 0
               AND (last_seen IS NULL OR last_seen < ?1)
               AND key NOT IN (SELECT DISTINCT issue_key FROM time_entries WHERE deleted_at IS NULL)",
            params![ts(&seen_before)],
        )?;
        Ok(n)
    }

    /// Downgrade issues that are no longer in any sprint but have entries, so the UI can show them under "Weitere".
    pub fn demote_unseen_issues(&self, seen_before: DateTime<Utc>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE issues SET source = 'tracked', sprint_id = NULL, sprint_name = NULL
             WHERE source IN ('sprint','selected-sprint') AND (last_seen IS NULL OR last_seen < ?1)",
            params![ts(&seen_before)],
        )?;
        Ok(n)
    }

    // --------------------------------------------------------- time entries

    fn entry_from_row(r: &Row<'_>) -> rusqlite::Result<TimeEntry> {
        let started: String = r.get(2)?;
        let ended: Option<String> = r.get(3)?;
        let created: String = r.get(11)?;
        let updated: String = r.get(12)?;
        let conv = |s: &str| {
            parse_ts(s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(e.to_string())),
                )
            })
        };
        Ok(TimeEntry {
            id: r.get(0)?,
            issue_key: r.get(1)?,
            started_at: conv(&started)?,
            ended_at: match ended {
                Some(e) => Some(conv(&e)?),
                None => None,
            },
            note: r.get(4)?,
            billable: r.get::<_, i32>(5)? != 0,
            sync_state: SyncState::parse(&r.get::<_, String>(6)?),
            jira_worklog_id: r.get(7)?,
            tempo_worklog_id: r.get(8)?,
            sync_error: r.get(9)?,
            booked_seconds: r.get(10)?,
            created_at: conv(&created)?,
            updated_at: conv(&updated)?,
        })
    }

    const ENTRY_COLUMNS: &'static str = "id, issue_key, started_at, ended_at, note, billable, sync_state, jira_worklog_id, tempo_worklog_id, sync_error, booked_seconds, created_at, updated_at";

    pub fn insert_entry(&self, input: &TimeEntryInput, now: DateTime<Utc>) -> Result<TimeEntry> {
        if let Some(end) = input.ended_at {
            if end < input.started_at {
                return Err(Error::Invalid(
                    crate::tr!("Ende liegt vor dem Start", "The end is before the start").into(),
                ));
            }
        }
        let id = {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO time_entries(issue_key, started_at, ended_at, note, billable, sync_state, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 'local', ?6, ?6)",
                params![
                    input.issue_key,
                    ts(&input.started_at),
                    input.ended_at.as_ref().map(ts),
                    input.note,
                    input.billable as i32,
                    ts(&now)
                ],
            )?;
            conn.last_insert_rowid()
        };
        self.get_entry(id)?.ok_or_else(|| {
            Error::Database(
                crate::tr!(
                    "Eintrag nach Einfügen nicht gefunden",
                    "Entry not found after insert"
                )
                .into(),
            )
        })
    }

    /// Insert an entry that already exists remotely (import).
    pub fn insert_imported_entry(
        &self,
        input: &TimeEntryInput,
        jira_worklog_id: Option<&str>,
        tempo_worklog_id: Option<&str>,
        booked_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<TimeEntry> {
        let id = {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO time_entries(issue_key, started_at, ended_at, note, billable, sync_state, jira_worklog_id, tempo_worklog_id, booked_seconds, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 'synced', ?6, ?7, ?8, ?9, ?9)",
                params![
                    input.issue_key,
                    ts(&input.started_at),
                    input.ended_at.as_ref().map(ts),
                    input.note,
                    input.billable as i32,
                    jira_worklog_id,
                    tempo_worklog_id,
                    booked_seconds,
                    ts(&now)
                ],
            )?;
            conn.last_insert_rowid()
        };
        self.get_entry(id)?.ok_or_else(|| {
            Error::Database(
                crate::tr!(
                    "Eintrag nach Einfügen nicht gefunden",
                    "Entry not found after insert"
                )
                .into(),
            )
        })
    }

    pub fn update_entry(
        &self,
        id: i64,
        input: &TimeEntryInput,
        now: DateTime<Utc>,
    ) -> Result<TimeEntry> {
        if let Some(end) = input.ended_at {
            if end < input.started_at {
                return Err(Error::Invalid(
                    crate::tr!("Ende liegt vor dem Start", "The end is before the start").into(),
                ));
            }
        }
        let current = self
            .get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))?;
        let changed = current.issue_key != input.issue_key
            || current.started_at != input.started_at
            || current.ended_at != input.ended_at
            || current.note != input.note
            || current.billable != input.billable;
        let new_state = match current.sync_state {
            SyncState::Synced if changed => SyncState::Modified,
            s => s,
        };
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE time_entries SET issue_key = ?2, started_at = ?3, ended_at = ?4, note = ?5, billable = ?6,
                    sync_state = ?7, updated_at = ?8 WHERE id = ?1",
                params![
                    id,
                    input.issue_key,
                    ts(&input.started_at),
                    input.ended_at.as_ref().map(ts),
                    input.note,
                    input.billable as i32,
                    new_state.as_str(),
                    ts(&now)
                ],
            )?;
        }
        self.get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))
    }

    pub fn get_entry(&self, id: i64) -> Result<Option<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE id = ?1 AND deleted_at IS NULL",
            Self::ENTRY_COLUMNS
        );
        Ok(conn
            .query_row(&sql, params![id], Self::entry_from_row)
            .optional()?)
    }

    /// Entries overlapping the half-open interval `[from, to)`. Running entries are included
    /// when they started before `to`.
    pub fn list_entries(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE deleted_at IS NULL AND started_at < ?2
               AND (ended_at IS NULL OR ended_at > ?1) ORDER BY started_at",
            Self::ENTRY_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![ts(&from), ts(&to)], Self::entry_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn running_entries(&self) -> Result<Vec<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE deleted_at IS NULL AND ended_at IS NULL ORDER BY started_at",
            Self::ENTRY_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::entry_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn entries_with_state(&self, state: SyncState) -> Result<Vec<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE deleted_at IS NULL AND ended_at IS NOT NULL AND sync_state = ?1 ORDER BY started_at",
            Self::ENTRY_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![state.as_str()], Self::entry_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn stop_entry(&self, id: i64, at: DateTime<Utc>, now: DateTime<Utc>) -> Result<TimeEntry> {
        let entry = self
            .get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))?;
        let at = if at < entry.started_at {
            entry.started_at
        } else {
            at
        };
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE time_entries SET ended_at = ?2, updated_at = ?3 WHERE id = ?1 AND ended_at IS NULL",
                params![id, ts(&at), ts(&now)],
            )?;
        }
        self.get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))
    }

    pub fn set_entry_note(&self, id: i64, note: &str, now: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE time_entries SET note = ?2, updated_at = ?3,
                sync_state = CASE WHEN sync_state = 'synced' THEN 'modified' ELSE sync_state END
             WHERE id = ?1",
            params![id, note, ts(&now)],
        )?;
        Ok(())
    }

    pub fn set_entry_start(
        &self,
        id: i64,
        started_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<TimeEntry> {
        let entry = self
            .get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))?;
        if let Some(end) = entry.ended_at {
            if started_at > end {
                return Err(Error::Invalid(
                    crate::tr!("Start liegt nach dem Ende", "The start is after the end").into(),
                ));
            }
        }
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE time_entries SET started_at = ?2, updated_at = ?3,
                    sync_state = CASE WHEN sync_state = 'synced' THEN 'modified' ELSE sync_state END
                 WHERE id = ?1",
                params![id, ts(&started_at), ts(&now)],
            )?;
        }
        self.get_entry(id)?
            .ok_or_else(|| Error::NotFound(crate::tr!("Eintrag {id}", "Entry {id}", id = id)))
    }

    pub fn soft_delete_entry(&self, id: i64, now: DateTime<Utc>) -> Result<Option<TimeEntry>> {
        let entry = self.get_entry(id)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE time_entries SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, ts(&now)],
        )?;
        Ok(entry)
    }

    pub fn restore_entry(&self, id: i64, now: DateTime<Utc>) -> Result<Option<TimeEntry>> {
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE time_entries SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1",
                params![id, ts(&now)],
            )?;
        }
        self.get_entry(id)
    }

    pub fn purge_deleted_entry(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM time_entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Soft-deleted entries that still have a remote worklog and need remote deletion.
    pub fn deleted_entries_with_remote(&self) -> Result<Vec<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE deleted_at IS NOT NULL AND (jira_worklog_id IS NOT NULL OR tempo_worklog_id IS NOT NULL)",
            Self::ENTRY_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::entry_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_sync_result(
        &self,
        id: i64,
        state: SyncState,
        jira_worklog_id: Option<&str>,
        tempo_worklog_id: Option<&str>,
        booked_seconds: Option<i64>,
        error: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE time_entries SET sync_state = ?2, jira_worklog_id = ?3, tempo_worklog_id = ?4,
                booked_seconds = ?5, sync_error = ?6, updated_at = ?7 WHERE id = ?1",
            params![
                id,
                state.as_str(),
                jira_worklog_id,
                tempo_worklog_id,
                booked_seconds,
                error,
                ts(&now)
            ],
        )?;
        Ok(())
    }

    pub fn set_sync_state(
        &self,
        id: i64,
        state: SyncState,
        error: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE time_entries SET sync_state = ?2, sync_error = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, state.as_str(), error, ts(&now)],
        )?;
        Ok(())
    }

    pub fn entry_by_jira_worklog(&self, worklog_id: &str) -> Result<Option<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE jira_worklog_id = ?1",
            Self::ENTRY_COLUMNS
        );
        Ok(conn
            .query_row(&sql, params![worklog_id], Self::entry_from_row)
            .optional()?)
    }

    pub fn entry_by_tempo_worklog(&self, worklog_id: &str) -> Result<Option<TimeEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {} FROM time_entries WHERE tempo_worklog_id = ?1",
            Self::ENTRY_COLUMNS
        );
        Ok(conn
            .query_row(&sql, params![worklog_id], Self::entry_from_row)
            .optional()?)
    }

    /// Issue keys of the most recently used entries, newest first, without duplicates.
    pub fn recent_issue_keys(&self, limit: usize) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT issue_key FROM time_entries WHERE deleted_at IS NULL GROUP BY issue_key ORDER BY MAX(started_at) DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Total tracked seconds per issue key (all time), used for budget bars.
    pub fn tracked_seconds_by_issue(
        &self,
        now: DateTime<Utc>,
    ) -> Result<std::collections::HashMap<String, i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT issue_key, started_at, ended_at FROM time_entries WHERE deleted_at IS NULL",
        )?;
        let mut map = std::collections::HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (key, s, e) = row?;
            let start = parse_ts(&s)?;
            let end = match e {
                Some(e) => parse_ts(&e)?,
                None => now,
            };
            *map.entry(key).or_insert(0) += (end - start).num_seconds().max(0);
        }
        Ok(map)
    }

    // -------------------------------------------------------------- backup

    /// Copy the database into `dir` as `yattt-YYYY-MM-DD.sqlite`, keeping at most `keep` files.
    pub fn backup(&self, dir: &Path, keep: usize, now: DateTime<Utc>) -> Result<Option<PathBuf>> {
        let Some(_) = &self.path else { return Ok(None) };
        std::fs::create_dir_all(dir)?;
        let target = dir.join(format!("yattt-{}.sqlite", now.format("%Y-%m-%d")));
        if target.exists() {
            return Ok(Some(target));
        }
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "VACUUM INTO ?1",
                params![target.to_string_lossy().to_string()],
            )?;
        }
        // rotate
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "sqlite").unwrap_or(false))
            .collect();
        files.sort();
        while files.len() > keep {
            let old = files.remove(0);
            let _ = std::fs::remove_file(old);
        }
        Ok(Some(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, h, m, 0).unwrap()
    }

    fn input(key: &str, start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> TimeEntryInput {
        TimeEntryInput {
            issue_key: key.into(),
            started_at: start,
            ended_at: end,
            note: "n".into(),
            billable: true,
        }
    }

    #[test]
    fn entry_lifecycle() {
        let store = Store::open_in_memory().unwrap();
        let now = t(9, 0);
        let e = store
            .insert_entry(&input("ABC-1", t(9, 0), None), now)
            .unwrap();
        assert!(e.is_running());
        assert_eq!(store.running_entries().unwrap().len(), 1);
        let stopped = store.stop_entry(e.id, t(10, 30), t(10, 30)).unwrap();
        assert_eq!(stopped.duration_seconds(t(12, 0)), 5400);
        assert!(store.running_entries().unwrap().is_empty());

        // overlapping query
        let list = store.list_entries(t(10, 0), t(11, 0)).unwrap();
        assert_eq!(list.len(), 1);
        let list = store.list_entries(t(11, 0), t(12, 0)).unwrap();
        assert!(list.is_empty());

        // synced -> modified on change
        store
            .set_sync_result(
                e.id,
                SyncState::Synced,
                Some("42"),
                None,
                Some(5400),
                None,
                now,
            )
            .unwrap();
        let updated = store
            .update_entry(e.id, &input("ABC-1", t(9, 0), Some(t(10, 45))), now)
            .unwrap();
        assert_eq!(updated.sync_state, SyncState::Modified);
        assert_eq!(store.entry_by_jira_worklog("42").unwrap().unwrap().id, e.id);

        // soft delete + restore
        store.soft_delete_entry(e.id, now).unwrap();
        assert!(store.get_entry(e.id).unwrap().is_none());
        assert_eq!(store.deleted_entries_with_remote().unwrap().len(), 1);
        store.restore_entry(e.id, now).unwrap();
        assert!(store.get_entry(e.id).unwrap().is_some());
    }

    #[test]
    fn rejects_end_before_start() {
        let store = Store::open_in_memory().unwrap();
        assert!(store
            .insert_entry(&input("ABC-1", t(9, 0), Some(t(8, 0))), t(9, 0))
            .is_err());
    }

    #[test]
    fn favorites_are_limited() {
        let store = Store::open_in_memory().unwrap();
        let mut issues = Vec::new();
        for n in 0..=MAX_FAVORITES {
            let mut i = sample_issue(&format!("ABC-{n}"));
            i.labels = vec![];
            issues.push(i);
        }
        store.upsert_issues(&issues, t(8, 0)).unwrap();
        for n in 0..MAX_FAVORITES {
            assert!(store.set_favorite(&format!("ABC-{n}"), true).unwrap());
        }
        assert_eq!(store.favorite_count().unwrap(), MAX_FAVORITES);
        // one more is refused …
        assert!(!store
            .set_favorite(&format!("ABC-{MAX_FAVORITES}"), true)
            .unwrap());
        assert_eq!(store.favorite_count().unwrap(), MAX_FAVORITES);
        // … re-marking an existing favorite is fine, and after removing one there is room again.
        assert!(store.set_favorite("ABC-0", true).unwrap());
        assert!(store.set_favorite("ABC-0", false).unwrap());
        assert!(store
            .set_favorite(&format!("ABC-{MAX_FAVORITES}"), true)
            .unwrap());
        let favs = store.favorite_issues().unwrap();
        assert_eq!(favs.len(), MAX_FAVORITES);
        assert!(favs.iter().all(|i| i.favorite));
    }

    fn sample_issue(key: &str) -> Issue {
        Issue {
            id: "1".into(),
            key: key.into(),
            project_key: key.rsplit_once('-').map(|(p, _)| p).unwrap_or("").into(),
            summary: "Test".into(),
            issue_type: "Story".into(),
            issue_type_id: None,
            issue_type_icon: None,
            is_subtask: false,
            status: "Open".into(),
            status_category: "new".into(),
            parent_key: None,
            epic_key: None,
            sprint_id: Some(5),
            sprint_name: Some("Sprint 5".into()),
            assignee: None,
            assignee_id: None,
            priority: None,
            due_date: None,
            original_estimate_s: Some(3600),
            remaining_estimate_s: None,
            time_spent_s: None,
            updated: None,
            labels: vec![],
            components: vec![],
            favorite: false,
            source: "sprint".into(),
            last_seen: None,
        }
    }

    #[test]
    fn issue_upsert_keeps_favorite_and_prunes() {
        let store = Store::open_in_memory().unwrap();
        let mut issue = Issue {
            id: "1".into(),
            key: "ABC-1".into(),
            project_key: "ABC".into(),
            summary: "Test".into(),
            issue_type: "Story".into(),
            issue_type_id: None,
            issue_type_icon: None,
            is_subtask: false,
            status: "Open".into(),
            status_category: "new".into(),
            parent_key: None,
            epic_key: None,
            sprint_id: Some(5),
            sprint_name: Some("Sprint 5".into()),
            assignee: None,
            assignee_id: None,
            priority: None,
            due_date: None,
            original_estimate_s: Some(3600),
            remaining_estimate_s: None,
            time_spent_s: None,
            updated: None,
            labels: vec!["team-a".into(), "backend".into()],
            components: vec!["Time Tracking".into()],
            favorite: false,
            source: "sprint".into(),
            last_seen: None,
        };
        store
            .upsert_issues(std::slice::from_ref(&issue), t(8, 0))
            .unwrap();
        let loaded = store.get_issue("ABC-1").unwrap().unwrap();
        assert_eq!(
            loaded.labels,
            vec!["team-a".to_string(), "backend".to_string()]
        );
        assert_eq!(loaded.components, vec!["Time Tracking".to_string()]);
        assert!(store.set_favorite("ABC-1", true).unwrap());
        issue.summary = "Changed".into();
        store
            .upsert_issues(std::slice::from_ref(&issue), t(9, 0))
            .unwrap();
        let loaded = store.get_issue("ABC-1").unwrap().unwrap();
        assert!(loaded.favorite);
        assert_eq!(loaded.summary, "Changed");

        // a second issue that is not favorite and not seen again gets pruned
        let mut other = issue.clone();
        other.key = "ABC-2".into();
        other.favorite = false;
        store.upsert_issues(&[other], t(8, 0)).unwrap();
        let removed = store.prune_stale_issues(t(8, 30)).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.list_issues().unwrap().len(), 1);
    }

    #[test]
    fn recent_keys_and_meta() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_entry(&input("A-1", t(8, 0), Some(t(9, 0))), t(9, 0))
            .unwrap();
        store
            .insert_entry(&input("B-2", t(9, 0), Some(t(10, 0))), t(10, 0))
            .unwrap();
        store
            .insert_entry(&input("A-1", t(10, 0), Some(t(11, 0))), t(11, 0))
            .unwrap();
        assert_eq!(
            store.recent_issue_keys(5).unwrap(),
            vec!["A-1".to_string(), "B-2".to_string()]
        );
        store.set_meta("k", "v").unwrap();
        assert_eq!(store.get_meta("k").unwrap().as_deref(), Some("v"));
        let totals = store.tracked_seconds_by_issue(t(12, 0)).unwrap();
        assert_eq!(totals["A-1"], 7200);
    }
}
