//! System tray / menu bar icon with quick actions.

use crate::state::AppState;
use chrono::Utc;
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use yattt_core::format_duration;

pub const TRAY_ID: &str = "main";

pub fn setup(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let menu = build_menu(app, &state)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("YaTTT")
        .show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let state_for_menu = state.clone();
    builder = builder
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref().to_string();
            handle_menu(app, &state_for_menu, &id);
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                // Left click opens the menu (show_menu_on_left_click); nothing else to do.
            }
        });
    builder.build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn handle_menu(app: &AppHandle, state: &Arc<AppState>, id: &str) {
    match id {
        "show" => show_main_window(app),
        "quit" => crate::request_quit(app, state),
        "refresh" => {
            let app = app.clone();
            let state = state.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::jobs::refresh_issues(&app, &state).await;
            });
        }
        "stop" => {
            let settings = state.settings();
            let engine = yattt_core::timer::TimerEngine::new(&state.store, &settings.timer);
            if let Err(e) = engine.stop_all(None, Utc::now()) {
                tracing::warn!("tray stop failed: {e}");
            }
            crate::after_timer_change(app, state);
        }
        other => {
            if let Some(key) = other.strip_prefix("start:") {
                start_issue(app, state, key);
            } else if let Some(key) = other.strip_prefix("fav:") {
                // A favorite toggles: running → stop that entry, otherwise start it.
                let running = state.store.running_entries().unwrap_or_default();
                match running.iter().find(|e| e.issue_key == key) {
                    Some(entry) => {
                        let settings = state.settings();
                        let engine =
                            yattt_core::timer::TimerEngine::new(&state.store, &settings.timer);
                        match engine.stop(entry.id, None, None, Utc::now()) {
                            Ok(_) => crate::after_timer_change(app, state),
                            Err(e) => tracing::warn!("tray stop failed: {e}"),
                        }
                    }
                    None => start_issue(app, state, key),
                }
            }
        }
    }
}

fn start_issue(app: &AppHandle, state: &Arc<AppState>, key: &str) {
    let settings = state.settings();
    let engine = yattt_core::timer::TimerEngine::new(&state.store, &settings.timer);
    match engine.start(key, "", None, Utc::now()) {
        Ok(_) => crate::after_timer_change(app, state),
        Err(e) => tracing::warn!("tray start failed: {e}"),
    }
}

fn build_menu(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<Menu<tauri::Wry>> {
    let settings = state.settings();
    let now = Utc::now();
    let running = state.store.running_entries().unwrap_or_default();
    let menu = Menu::new(app)?;
    if running.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "status",
            yattt_core::tr!("Kein Timer läuft", "No timer running"),
            false,
            None::<&str>,
        )?)?;
    } else {
        for entry in &running {
            let summary = state
                .store
                .get_issue(&entry.issue_key)
                .ok()
                .flatten()
                .map(|i| i.summary)
                .unwrap_or_default();
            let label = format!(
                "{}  {} · {}",
                format_duration(entry.duration_seconds(now), settings.work.time_format),
                entry.issue_key,
                truncate(&summary, 36)
            );
            menu.append(&MenuItem::with_id(
                app,
                format!("running:{}", entry.id),
                label,
                false,
                None::<&str>,
            )?)?;
        }
        menu.append(&MenuItem::with_id(
            app,
            "stop",
            yattt_core::tr!("Timer stoppen", "Stop timer"),
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    // Favorites: a check mark shows which one is running; a click starts or stops it.
    let favorites = state.store.favorite_issues().unwrap_or_default();
    if !favorites.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "favorites",
            yattt_core::tr!("Favoriten", "Favorites"),
            false,
            None::<&str>,
        )?)?;
        for issue in &favorites {
            let is_running = running.iter().any(|e| e.issue_key == issue.key);
            let label = format!("{}  {}", issue.key, truncate(&issue.summary, 40));
            menu.append(&CheckMenuItem::with_id(
                app,
                format!("fav:{}", issue.key),
                label,
                true,
                is_running,
                None::<&str>,
            )?)?;
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    let recent = state.store.recent_issue_keys(10).unwrap_or_default();
    let mut added = 0;
    for key in recent {
        if running.iter().any(|e| e.issue_key == key) || favorites.iter().any(|f| f.key == key) {
            continue;
        }
        let summary = state
            .store
            .get_issue(&key)
            .ok()
            .flatten()
            .map(|i| i.summary)
            .unwrap_or_default();
        let label = format!("▶ {}  {}", key, truncate(&summary, 40));
        menu.append(&MenuItem::with_id(
            app,
            format!("start:{key}"),
            label,
            true,
            None::<&str>,
        )?)?;
        added += 1;
        if added >= 5 {
            break;
        }
    }
    if added > 0 {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "show",
        yattt_core::tr!("YaTTT anzeigen", "Show YaTTT"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "refresh",
        yattt_core::tr!("Tickets aktualisieren", "Refresh tickets"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        yattt_core::tr!("YaTTT beenden", "Quit YaTTT"),
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Rebuild the menu and update the title (macOS shows text next to the icon).
pub fn refresh(app: &AppHandle, state: &Arc<AppState>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app, state) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => tracing::warn!("tray menu failed: {e}"),
    }
    update_title(app, state);
}

/// Only the title changes every few seconds, so this is cheap enough to call from the tick loop.
pub fn update_title(app: &AppHandle, state: &Arc<AppState>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let settings = state.settings();
    let running = state.store.running_entries().unwrap_or_default();
    if settings.behavior.show_timer_in_tray && !running.is_empty() {
        let now = Utc::now();
        let total: i64 = running.iter().map(|e| e.duration_seconds(now)).sum();
        let text = if running.len() == 1 {
            format!(
                "{} {}",
                format_duration(total, settings.work.time_format),
                running[0].issue_key
            )
        } else {
            yattt_core::tr!(
                "{d} ({n} Timer)",
                "{d} ({n} timers)",
                d = format_duration(total, settings.work.time_format),
                n = running.len()
            )
        };
        let _ = tray.set_title(Some(text.clone()));
        let _ = tray.set_tooltip(Some(format!("YaTTT – {text}")));
    } else {
        let _ = tray.set_title(None::<&str>);
        let _ = tray.set_tooltip(Some("YaTTT"));
    }
}
