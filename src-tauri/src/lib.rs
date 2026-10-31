//! YaTTT desktop application (Tauri 2). All business logic lives in `yattt-core`.

mod commands;
mod jobs;
mod login;
mod scheduler;
mod shutdown;
mod state;
mod tray;

use chrono::Utc;
use state::{AppState, RestorePrompt};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use yattt_core::settings::{AppPaths, Settings};

pub use jobs::emit;

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

fn init_logging(paths: &AppPaths) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    let log_dir = paths.data_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "yattt.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,yattt=debug,yattt_core=debug"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(file_writer));
    #[cfg(debug_assertions)]
    let registry = registry.with(fmt::layer().with_writer(std::io::stderr));
    let _ = registry.try_init();
}

/// Called after any change to timers/entries: tray and frontend get notified.
pub fn after_timer_change(app: &AppHandle, state: &Arc<AppState>) {
    tray::refresh(app, state);
    emit(app, "timer-changed", ());
    emit(app, "entries-changed", ());
}

/// Quit requested by the user (tray, menu, window close without tray). Asks first when a timer runs.
pub fn request_quit(app: &AppHandle, state: &Arc<AppState>) {
    if state.running_count() > 0 && !state.exit_confirmed() {
        tray::show_main_window(app);
        emit(app, "exit-requested", "quit");
        return;
    }
    state.write_heartbeat(Utc::now());
    state
        .exit_confirmed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

fn register_shortcuts(app: &AppHandle, state: &Arc<AppState>, settings: &Settings) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if !settings.behavior.shortcut_toggle_timer.trim().is_empty() {
        match Shortcut::from_str(settings.behavior.shortcut_toggle_timer.trim()) {
            Ok(shortcut) => {
                let state = state.clone();
                if let Err(e) = gs.on_shortcut(shortcut, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_timer(app, &state);
                    }
                }) {
                    tracing::warn!("shortcut toggle registration failed: {e}");
                }
            }
            Err(e) => tracing::warn!(
                "invalid shortcut '{}': {e}",
                settings.behavior.shortcut_toggle_timer
            ),
        }
    }
    if !settings.behavior.shortcut_show_window.trim().is_empty() {
        match Shortcut::from_str(settings.behavior.shortcut_show_window.trim()) {
            Ok(shortcut) => {
                if let Err(e) = gs.on_shortcut(shortcut, move |app, _sc, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                }) {
                    tracing::warn!("shortcut show registration failed: {e}");
                }
            }
            Err(e) => tracing::warn!(
                "invalid shortcut '{}': {e}",
                settings.behavior.shortcut_show_window
            ),
        }
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let visible = w.is_visible().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        if visible && focused {
            let _ = w.hide();
        } else {
            tray::show_main_window(app);
        }
    }
}

/// Global shortcut: stop all running timers, or restart the most recent issue.
fn toggle_timer(app: &AppHandle, state: &Arc<AppState>) {
    let settings = state.settings();
    let engine = yattt_core::timer::TimerEngine::new(&state.store, &settings.timer);
    let running = state.store.running_entries().unwrap_or_default();
    let result = if running.is_empty() {
        match state
            .store
            .recent_issue_keys(1)
            .ok()
            .and_then(|v| v.into_iter().next())
        {
            Some(key) => engine.start(&key, "", None, Utc::now()).map(|r| {
                scheduler::notify(
                    app,
                    state,
                    "Timer gestartet",
                    &format!("{} läuft", r.started.issue_key),
                );
            }),
            None => {
                tray::show_main_window(app);
                Ok(())
            }
        }
    } else {
        engine.stop_all(None, Utc::now()).map(|_| {
            scheduler::notify(app, state, "Timer gestoppt", "Alle Timer wurden gestoppt.");
        })
    };
    if let Err(e) = result {
        tracing::warn!("toggle timer failed: {e}");
    }
    after_timer_change(app, state);
}

/// Apply settings that have side effects outside the core (autostart, shortcuts, tray).
pub fn apply_settings(app: &AppHandle, state: &Arc<AppState>, old: &Settings, new: &Settings) {
    if old.behavior.autostart != new.behavior.autostart {
        let launcher = app.autolaunch();
        let res = if new.behavior.autostart {
            launcher.enable()
        } else {
            launcher.disable()
        };
        if let Err(e) = res {
            tracing::warn!("autostart change failed: {e}");
        }
    }
    if old.behavior.shortcut_toggle_timer != new.behavior.shortcut_toggle_timer
        || old.behavior.shortcut_show_window != new.behavior.shortcut_show_window
    {
        register_shortcuts(app, state, new);
    }
    tray::update_title(app, state);
}

fn prepare_restore_prompt(state: &Arc<AppState>) {
    let running = state.store.running_entries().unwrap_or_default();
    if running.is_empty() {
        let _ = state.store.delete_meta("kept_running_on_exit");
        return;
    }
    let kept = state
        .store
        .get_meta("kept_running_on_exit")
        .ok()
        .flatten()
        .is_some();
    let _ = state.store.delete_meta("kept_running_on_exit");
    let last_seen = state.last_heartbeat();
    *state.restore.lock().unwrap() = Some(RestorePrompt {
        entries: running,
        last_seen,
        kept_running_on_exit: kept,
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = AppPaths::discover().expect("Benutzerverzeichnisse nicht gefunden");
    init_logging(&paths);
    let state = Arc::new(
        AppState::init(paths).expect("Anwendungsdaten konnten nicht initialisiert werden"),
    );
    let state_for_run = state.clone();
    let state_for_setup = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = state_for_setup;
            tray::setup(&handle, state.clone())?;
            prepare_restore_prompt(&state);
            shutdown::install(&handle, state.clone());
            let settings = state.settings();
            register_shortcuts(&handle, &state, &settings);
            if std::env::args().any(|a| a == "--minimized") && settings.behavior.close_to_tray {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            scheduler::start(handle.clone(), state.clone());
            // Developer aid: `YATTT_DEV_LOGIN=1 npm run tauri dev` opens the browser login right away
            // so the SSO flow can be exercised without clicking through the UI. Debug builds only.
            if cfg!(debug_assertions) && std::env::var_os("YATTT_DEV_LOGIN").is_some() {
                let h = handle.clone();
                let s = state.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    match login::browser_login(h, s).await {
                        Ok(status) => tracing::info!(
                            "dev login finished: {:?}",
                            status.user.map(|u| u.display_name)
                        ),
                        Err(e) => tracing::warn!("dev login ended: {e}"),
                    }
                });
            }
            tauri::async_runtime::spawn(async move {
                jobs::restore_session(&handle, &state).await;
                if state.is_authenticated() {
                    let _ = jobs::refresh_issues(&handle, &state).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let state = window.state::<Arc<AppState>>();
                    if state.settings().behavior.close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else if state.running_count() > 0 && !state.exit_confirmed() {
                        api.prevent_close();
                        request_quit(window.app_handle(), &state);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_auth_status,
            commands::start_browser_login,
            commands::cancel_browser_login,
            commands::login_with_password,
            commands::login_with_token,
            commands::logout,
            commands::search_projects,
            commands::search_sprints,
            commands::list_issue_types,
            commands::set_project_selection,
            commands::set_sprint_selection,
            commands::get_issues,
            commands::refresh_issues,
            commands::get_refresh_status,
            commands::set_favorite,
            commands::search_labels,
            commands::add_issue_by_key,
            commands::set_project_color,
            commands::get_issue_url,
            commands::start_timer,
            commands::stop_timer,
            commands::get_running,
            commands::update_running,
            commands::list_entries,
            commands::add_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::restore_entry,
            commands::book_entries,
            commands::sync_now,
            commands::import_worklogs,
            commands::get_stats,
            commands::export_entries,
            commands::get_export_rows,
            commands::get_idle_prompt,
            commands::respond_idle,
            commands::get_restore_prompt,
            commands::respond_restore,
            commands::request_quit,
            commands::confirm_exit,
            commands::cancel_exit,
            commands::set_tempo_cloud_token,
            commands::get_app_info,
        ])
        .build(tauri::generate_context!())
        .expect("Fehler beim Start von YaTTT")
        .run(move |app, event| match event {
            RunEvent::ExitRequested { api, code, .. } => {
                if code.is_none()
                    && !state_for_run.exit_confirmed()
                    && state_for_run.running_count() > 0
                {
                    api.prevent_exit();
                    tray::show_main_window(app);
                    emit(app, "exit-requested", "quit");
                }
            }
            RunEvent::Exit => {
                state_for_run.write_heartbeat(Utc::now());
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                tray::show_main_window(app);
            }
            _ => {}
        });
}
