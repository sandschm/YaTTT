//! OS shutdown / logout handling for running timers.
//!
//! * macOS: `applicationShouldTerminate:` is added to tao's application delegate and shows a
//!   native alert ("stoppen" / "weiterlaufen lassen" / "abbrechen").
//! * Windows: the main window is subclassed; `WM_QUERYENDSESSION` registers a shutdown block
//!   reason and asks in the app. `WM_ENDSESSION` persists the state.
//! * Linux: no reliable dialog during shutdown; the heartbeat plus the restore prompt at the
//!   next start cover this case.
//!
//! On every platform the heartbeat written by the scheduler is the actual safety net.

use crate::state::AppState;
use chrono::Utc;
use std::sync::Arc;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownChoice {
    StopAndQuit,
    KeepAndQuit,
    Cancel,
}

pub fn install(app: &AppHandle, state: Arc<AppState>) {
    #[cfg(target_os = "macos")]
    macos::install(app.clone(), state);
    #[cfg(target_os = "windows")]
    windows::install(app, state);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (app, state);
    }
}

/// Apply the user's choice. Returns `true` when the app may terminate.
pub fn apply_choice(state: &AppState, choice: ShutdownChoice) -> bool {
    let now = Utc::now();
    match choice {
        ShutdownChoice::StopAndQuit => {
            let settings = state.settings();
            let engine = yattt_core::timer::TimerEngine::new(&state.store, &settings.timer);
            if let Err(e) = engine.stop_all(None, now) {
                tracing::warn!("stop_all on exit failed: {e}");
            }
            let _ = state.store.delete_meta("kept_running_on_exit");
            state.write_heartbeat(now);
            state
                .exit_confirmed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
        ShutdownChoice::KeepAndQuit => {
            let _ = state.store.set_meta("kept_running_on_exit", "1");
            state.write_heartbeat(now);
            state
                .exit_confirmed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
        ShutdownChoice::Cancel => false,
    }
}

pub fn clear_block_reason(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    windows::clear_block_reason(app);
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
    }
}
