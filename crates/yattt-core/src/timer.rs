//! Timer engine: start/stop/switch and idle handling on top of the store.
//! A running timer is simply a time entry without `ended_at`.

use crate::error::{Error, Result};
use crate::models::*;
use crate::settings::TimerSettings;
use crate::storage::Store;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub started: TimeEntry,
    /// Entries that were stopped because only one timer is allowed.
    pub stopped: Vec<TimeEntry>,
    /// Stopped entries that were discarded because they were too short.
    pub discarded: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    /// `None` if the entry was discarded (too short).
    pub entry: Option<TimeEntry>,
    pub discarded: bool,
}

pub struct TimerEngine<'a> {
    pub store: &'a Store,
    pub settings: &'a TimerSettings,
}

impl<'a> TimerEngine<'a> {
    pub fn new(store: &'a Store, settings: &'a TimerSettings) -> Self {
        Self { store, settings }
    }

    pub fn running(&self) -> Result<Vec<TimeEntry>> {
        self.store.running_entries()
    }

    /// Start a timer for `issue_key`. In single-timer mode, running timers are stopped first.
    pub fn start(
        &self,
        issue_key: &str,
        note: &str,
        at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<StartResult> {
        let at = at.unwrap_or(now);
        if at > now + Duration::minutes(1) {
            return Err(Error::Invalid("Startzeit liegt in der Zukunft".into()));
        }
        let mut stopped = Vec::new();
        let mut discarded = Vec::new();
        let running = self.store.running_entries()?;
        if running.iter().any(|e| e.issue_key == issue_key) {
            return Err(Error::Invalid(format!(
                "Für {issue_key} läuft bereits ein Timer"
            )));
        }
        if !self.settings.allow_concurrent {
            for entry in running {
                let res = self.stop(entry.id, Some(at), None, now)?;
                match res.entry {
                    Some(e) => stopped.push(e),
                    None => discarded.push(entry.id),
                }
            }
        }
        let input = TimeEntryInput {
            issue_key: issue_key.to_string(),
            started_at: at,
            ended_at: None,
            note: note.to_string(),
            billable: self.settings.default_billable,
        };
        let started = self.store.insert_entry(&input, now)?;
        Ok(StartResult {
            started,
            stopped,
            discarded,
        })
    }

    /// Stop a running entry. Entries shorter than the configured minimum are discarded.
    pub fn stop(
        &self,
        entry_id: i64,
        at: Option<DateTime<Utc>>,
        note: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<StopResult> {
        let entry = self
            .store
            .get_entry(entry_id)?
            .ok_or_else(|| Error::NotFound(format!("Eintrag {entry_id}")))?;
        if !entry.is_running() {
            return Ok(StopResult {
                entry: Some(entry),
                discarded: false,
            });
        }
        let at = at.unwrap_or(now).min(now);
        if let Some(n) = note {
            self.store.set_entry_note(entry_id, n, now)?;
        }
        let stopped = self.store.stop_entry(entry_id, at, now)?;
        let min = self.settings.ignore_short_entries_seconds as i64;
        if min > 0 && stopped.duration_seconds(now) < min {
            self.store.purge_deleted_entry(entry_id)?;
            return Ok(StopResult {
                entry: None,
                discarded: true,
            });
        }
        Ok(StopResult {
            entry: Some(stopped),
            discarded: false,
        })
    }

    pub fn stop_all(
        &self,
        at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Vec<StopResult>> {
        let mut out = Vec::new();
        for entry in self.store.running_entries()? {
            out.push(self.stop(entry.id, at, None, now)?);
        }
        Ok(out)
    }

    /// Apply the user's decision after an idle period that began at `idle_started_at`.
    pub fn apply_idle_decision(
        &self,
        entry_id: i64,
        idle_started_at: DateTime<Utc>,
        decision: IdleDecision,
        now: DateTime<Utc>,
    ) -> Result<Option<TimeEntry>> {
        let entry = self
            .store
            .get_entry(entry_id)?
            .ok_or_else(|| Error::NotFound(format!("Eintrag {entry_id}")))?;
        if !entry.is_running() {
            return Ok(Some(entry));
        }
        match decision {
            IdleDecision::Keep => Ok(Some(entry)),
            IdleDecision::StopAtIdleStart => {
                let res = self.stop(entry_id, Some(idle_started_at), None, now)?;
                Ok(res.entry)
            }
            IdleDecision::DiscardIdle => {
                // Close the old entry at idle start and continue with a fresh one from now.
                let res = self.stop(entry_id, Some(idle_started_at), None, now)?;
                let input = TimeEntryInput {
                    issue_key: entry.issue_key.clone(),
                    started_at: now,
                    ended_at: None,
                    note: entry.note.clone(),
                    billable: entry.billable,
                };
                let fresh = self.store.insert_entry(&input, now)?;
                let _ = res;
                Ok(Some(fresh))
            }
        }
    }

    /// Apply the decision for a timer that was running when the app last ended.
    pub fn apply_restore_decision(
        &self,
        entry_id: i64,
        last_seen: Option<DateTime<Utc>>,
        decision: RestoreDecision,
        now: DateTime<Utc>,
    ) -> Result<Option<TimeEntry>> {
        match decision {
            RestoreDecision::KeepRunning => self.store.get_entry(entry_id),
            RestoreDecision::StopAtLastSeen => {
                let at = last_seen.unwrap_or(now);
                Ok(self.stop(entry_id, Some(at), None, now)?.entry)
            }
            RestoreDecision::StopNow => Ok(self.stop(entry_id, Some(now), None, now)?.entry),
            RestoreDecision::Discard => {
                self.store.purge_deleted_entry(entry_id)?;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, h, m, 0).unwrap()
    }

    #[test]
    fn single_timer_mode_switches() {
        let store = Store::open_in_memory().unwrap();
        let settings = TimerSettings {
            ignore_short_entries_seconds: 0,
            ..Default::default()
        };
        let engine = TimerEngine::new(&store, &settings);
        let a = engine.start("A-1", "", None, t(9, 0)).unwrap();
        assert!(a.stopped.is_empty());
        let b = engine.start("B-1", "", None, t(10, 0)).unwrap();
        assert_eq!(b.stopped.len(), 1);
        assert_eq!(b.stopped[0].duration_seconds(t(10, 0)), 3600);
        assert_eq!(engine.running().unwrap().len(), 1);
        assert!(engine.start("B-1", "", None, t(10, 5)).is_err());
    }

    #[test]
    fn concurrent_mode_keeps_both() {
        let store = Store::open_in_memory().unwrap();
        let settings = TimerSettings {
            allow_concurrent: true,
            ..Default::default()
        };
        let engine = TimerEngine::new(&store, &settings);
        engine.start("A-1", "", None, t(9, 0)).unwrap();
        engine.start("B-1", "", None, t(9, 5)).unwrap();
        assert_eq!(engine.running().unwrap().len(), 2);
        let stopped = engine.stop_all(None, t(11, 0)).unwrap();
        assert_eq!(stopped.len(), 2);
        assert!(engine.running().unwrap().is_empty());
    }

    #[test]
    fn short_entries_are_discarded() {
        let store = Store::open_in_memory().unwrap();
        let settings = TimerSettings {
            ignore_short_entries_seconds: 60,
            ..Default::default()
        };
        let engine = TimerEngine::new(&store, &settings);
        let a = engine.start("A-1", "", None, t(9, 0)).unwrap();
        let res = engine
            .stop(a.started.id, None, None, t(9, 0) + Duration::seconds(30))
            .unwrap();
        assert!(res.discarded);
        assert!(store.get_entry(a.started.id).unwrap().is_none());
    }

    #[test]
    fn idle_decisions() {
        let store = Store::open_in_memory().unwrap();
        let settings = TimerSettings {
            ignore_short_entries_seconds: 0,
            ..Default::default()
        };
        let engine = TimerEngine::new(&store, &settings);
        let a = engine.start("A-1", "note", None, t(9, 0)).unwrap();
        // discard idle: old entry closed at 9:30, new entry from 10:00
        let fresh = engine
            .apply_idle_decision(a.started.id, t(9, 30), IdleDecision::DiscardIdle, t(10, 0))
            .unwrap()
            .unwrap();
        assert!(fresh.is_running());
        assert_eq!(fresh.started_at, t(10, 0));
        assert_eq!(fresh.note, "note");
        let old = store.get_entry(a.started.id).unwrap().unwrap();
        assert_eq!(old.ended_at, Some(t(9, 30)));
        // stop at idle start
        let stopped = engine
            .apply_idle_decision(
                fresh.id,
                t(10, 20),
                IdleDecision::StopAtIdleStart,
                t(10, 40),
            )
            .unwrap()
            .unwrap();
        assert_eq!(stopped.ended_at, Some(t(10, 20)));
    }

    #[test]
    fn restore_decisions() {
        let store = Store::open_in_memory().unwrap();
        let settings = TimerSettings {
            ignore_short_entries_seconds: 0,
            ..Default::default()
        };
        let engine = TimerEngine::new(&store, &settings);
        let a = engine.start("A-1", "", None, t(9, 0)).unwrap();
        let e = engine
            .apply_restore_decision(
                a.started.id,
                Some(t(17, 0)),
                RestoreDecision::StopAtLastSeen,
                t(23, 0),
            )
            .unwrap()
            .unwrap();
        assert_eq!(e.ended_at, Some(t(17, 0)));
        let b = engine.start("B-1", "", None, t(23, 0)).unwrap();
        assert!(engine
            .apply_restore_decision(b.started.id, None, RestoreDecision::Discard, t(23, 30))
            .unwrap()
            .is_none());
    }
}
