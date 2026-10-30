//! Background loops: heartbeat, idle/sleep detection, auto refresh, sync, reminders, backups.

use crate::jobs::{emit, has_pending_work, refresh_issues, sync_pending};
use crate::state::{AppState, IdlePrompt};
use chrono::{Duration as ChronoDuration, Local, NaiveTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use yattt_core::format_duration;

const TICK: Duration = Duration::from_secs(5);
const HEARTBEAT_EVERY_SECS: i64 = 15;
/// If two ticks are further apart than this, the machine was asleep (or the process was frozen).
const SLEEP_GAP_SECS: i64 = 180;

pub fn start(app: AppHandle, state: Arc<AppState>) {
    {
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn(async move { tick_loop(app, state).await });
    }
    {
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn(async move { refresh_loop(app, state).await });
    }
    {
        let app = app.clone();
        let state = state.clone();
        tauri::async_runtime::spawn(async move { sync_loop(app, state).await });
    }
    tauri::async_runtime::spawn(async move { backup_loop(state).await });
    let _ = app;
}

pub fn notify(app: &AppHandle, state: &AppState, title: &str, body: &str) {
    if !state.settings().behavior.notifications {
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("notification failed: {e}");
    }
}

async fn tick_loop(app: AppHandle, state: Arc<AppState>) {
    let mut last_heartbeat = Utc::now() - ChronoDuration::seconds(HEARTBEAT_EVERY_SECS);
    loop {
        tokio::time::sleep(TICK).await;
        let now = Utc::now();
        let running = state.store.running_entries().unwrap_or_default();
        let settings = state.settings();

        // Heartbeat so that a crash/shutdown leaves a last-known-alive timestamp.
        if (now - last_heartbeat).num_seconds() >= HEARTBEAT_EVERY_SECS {
            state.write_heartbeat(now);
            last_heartbeat = now;
        }

        // Sleep detection via time jump, then idle detection via last input.
        // The tracker lock is released before anything is emitted or awaited.
        let prompt_to_emit: Option<IdlePrompt> = {
            let mut tracker = state.idle.lock().unwrap();
            let gap = tracker
                .last_tick
                .map(|t| (now - t).num_seconds())
                .unwrap_or(0);
            tracker.last_tick = Some(now);
            if running.is_empty() {
                tracker.idle_since = None;
                None
            } else if tracker.prompt.is_some() {
                None
            } else if gap > SLEEP_GAP_SECS {
                let since = now - ChronoDuration::seconds(gap);
                let prompt = IdlePrompt {
                    entry_ids: running.iter().map(|e| e.id).collect(),
                    idle_since: since,
                    reason: "sleep".into(),
                };
                tracker.prompt = Some(prompt.clone());
                tracker.idle_since = None;
                Some(prompt)
            } else {
                let timeout = settings.timer.idle_timeout_minutes as i64 * 60;
                if timeout == 0 {
                    None
                } else {
                    match yattt_core::idle::idle_seconds() {
                        Some(idle) if idle as i64 >= timeout => {
                            if tracker.idle_since.is_none() {
                                tracker.idle_since =
                                    Some(now - ChronoDuration::seconds(idle as i64));
                            }
                            None
                        }
                        Some(_) => tracker.idle_since.take().map(|since| {
                            let prompt = IdlePrompt {
                                entry_ids: running.iter().map(|e| e.id).collect(),
                                idle_since: since,
                                reason: "idle".into(),
                            };
                            tracker.prompt = Some(prompt.clone());
                            prompt
                        }),
                        None => None,
                    }
                }
            }
        };
        if let Some(prompt) = prompt_to_emit {
            emit(&app, "idle-prompt", prompt);
            crate::tray::show_main_window(&app);
        }

        if !running.is_empty() {
            crate::tray::update_title(&app, &state);
        }
        reminders(&app, &state, now);
    }
}

fn reminders(app: &AppHandle, state: &Arc<AppState>, now: chrono::DateTime<Utc>) {
    let settings = state.settings();
    if !settings.behavior.notifications {
        return;
    }
    let local_now = now.with_timezone(&Local);
    let today = local_now.date_naive();
    let running = state.store.running_entries().unwrap_or_default();
    let weekday_idx = chrono::Datelike::weekday(&today).num_days_from_monday() as usize;
    let target_today = (settings
        .work
        .hours_per_weekday
        .get(weekday_idx)
        .copied()
        .unwrap_or(0.0)
        * 3600.0) as i64;
    let mut rem = state.reminders.lock().unwrap();

    // No timer running after the configured start of the workday.
    if target_today > 0
        && running.is_empty()
        && !settings.work.workday_start.is_empty()
        && rem.last_no_timer_reminder != Some(today)
    {
        if let Ok(start) = NaiveTime::parse_from_str(&settings.work.workday_start, "%H:%M") {
            let threshold = start + ChronoDuration::minutes(15);
            if local_now.time() >= threshold
                && local_now.time() < threshold + ChronoDuration::minutes(90)
            {
                let day_start = Local
                    .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .map(|d| d.with_timezone(&Utc));
                let tracked_today = day_start
                    .and_then(|ds| state.store.list_entries(ds, now).ok())
                    .map(|v| v.iter().map(|e| e.duration_seconds(now)).sum::<i64>())
                    .unwrap_or(0);
                if tracked_today == 0 {
                    rem.last_no_timer_reminder = Some(today);
                    notify(
                        app,
                        state,
                        "Noch kein Timer gestartet",
                        "Der Arbeitstag hat begonnen, aber es läuft keine Zeiterfassung.",
                    );
                }
            }
        }
    }

    // Daily goal reached.
    if settings.work.daily_goal_notification
        && target_today > 0
        && rem.last_goal_reminder != Some(today)
    {
        if let Some(ds) = Local
            .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .map(|d| d.with_timezone(&Utc))
        {
            let tracked_today: i64 = state
                .store
                .list_entries(ds, now)
                .map(|v| {
                    v.iter()
                        .map(|e| {
                            e.duration_seconds(now)
                                .min((now - e.started_at.max(ds)).num_seconds().max(0))
                        })
                        .sum()
                })
                .unwrap_or(0);
            if tracked_today >= target_today {
                rem.last_goal_reminder = Some(today);
                notify(
                    app,
                    state,
                    "Tagesziel erreicht",
                    &format!(
                        "Heute wurden {} Stunden erfasst.",
                        format_duration(tracked_today, settings.work.time_format)
                    ),
                );
            }
        }
    }

    // Break reminder: continuous tracking longer than configured.
    if settings.work.break_reminder_after_minutes > 0 && !running.is_empty() {
        let oldest = running.iter().map(|e| e.started_at).min().unwrap();
        let continuous = (now - oldest).num_minutes();
        let due = rem
            .last_break_reminder
            .map(|t| (now - t).num_minutes() >= settings.work.break_reminder_after_minutes as i64)
            .unwrap_or(true);
        if continuous >= settings.work.break_reminder_after_minutes as i64 && due {
            rem.last_break_reminder = Some(now);
            notify(
                app,
                state,
                "Zeit für eine Pause",
                &format!(
                    "Der Timer läuft seit {}.",
                    format_duration(continuous * 60, settings.work.time_format)
                ),
            );
        }
    }

    // Long-running timer warning (forgot to stop?).
    if settings.timer.long_running_warning_hours > 0 && !running.is_empty() {
        let limit = settings.timer.long_running_warning_hours as i64 * 3600;
        if let Some(e) = running.iter().find(|e| e.duration_seconds(now) > limit) {
            let due = rem
                .last_long_running_warning
                .map(|t| (now - t).num_hours() >= 1)
                .unwrap_or(true);
            if due {
                rem.last_long_running_warning = Some(now);
                notify(
                    app,
                    state,
                    "Timer läuft sehr lange",
                    &format!(
                        "{} läuft seit {}. Vergessen zu stoppen?",
                        e.issue_key,
                        format_duration(e.duration_seconds(now), settings.work.time_format)
                    ),
                );
            }
        }
    }

    // Unbooked entries from previous days.
    if settings.work.unbooked_reminder
        && rem.last_unbooked_reminder != Some(today)
        && local_now.time() >= NaiveTime::from_hms_opt(16, 0, 0).unwrap()
    {
        if let Some(ds) = Local
            .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .map(|d| d.with_timezone(&Utc))
        {
            let old = state
                .store
                .list_entries(ds - ChronoDuration::days(14), ds)
                .map(|v| {
                    v.into_iter()
                        .filter(|e| {
                            !e.is_running() && e.sync_state != yattt_core::models::SyncState::Synced
                        })
                        .count()
                })
                .unwrap_or(0);
            if old > 0 {
                rem.last_unbooked_reminder = Some(today);
                notify(
                    app,
                    state,
                    "Ungebuchte Zeiten",
                    &format!(
                        "{old} Einträge der letzten zwei Wochen sind noch nicht nach Jira gebucht."
                    ),
                );
            }
        }
    }
}

use chrono::TimeZone;

async fn refresh_loop(app: AppHandle, state: Arc<AppState>) {
    // First refresh shortly after start, when a session could be restored.
    tokio::time::sleep(Duration::from_secs(3)).await;
    loop {
        let settings = state.settings();
        let due = {
            let status = state.refresh.read().unwrap();
            settings.refresh.auto
                && state.is_authenticated()
                && !status.in_progress
                && status
                    .last_success
                    .map(|t| {
                        (Utc::now() - t).num_seconds()
                            >= settings.refresh.interval_minutes.max(1) as i64 * 60
                    })
                    .unwrap_or(true)
        };
        if due {
            if let Err(e) = refresh_issues(&app, &state).await {
                tracing::info!("auto refresh failed: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

async fn sync_loop(app: AppHandle, state: Arc<AppState>) {
    tokio::time::sleep(Duration::from_secs(10)).await;
    loop {
        if state.is_authenticated() && has_pending_work(&state) {
            if let Err(e) = sync_pending(&app, &state).await {
                tracing::info!("background sync failed: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn backup_loop(state: Arc<AppState>) {
    loop {
        match state
            .store
            .backup(&state.paths.backup_dir(), 14, Utc::now())
        {
            Ok(Some(p)) => tracing::info!("backup written to {}", p.display()),
            Ok(None) => {}
            Err(e) => tracing::warn!("backup failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}
