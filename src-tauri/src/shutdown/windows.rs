//! Windows: subclass the main window to see `WM_QUERYENDSESSION` / `WM_ENDSESSION`.
//! A message box cannot be shown at that point (Windows displays its own blocking screen), so we
//! register a shutdown block reason, return FALSE once and let the user decide in the app.

use crate::state::AppState;
use chrono::Utc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};

static CTX: OnceLock<(AppHandle, Arc<AppState>)> = OnceLock::new();
const SUBCLASS_ID: usize = 0x5941_5454;

fn main_hwnd(app: &AppHandle) -> Option<HWND> {
    let window = app.get_webview_window("main")?;
    let raw = window.hwnd().ok()?;
    Some(HWND(raw.0 as *mut core::ffi::c_void))
}

pub fn install(app: &AppHandle, state: Arc<AppState>) {
    let _ = CTX.set((app.clone(), state));
    let Some(hwnd) = main_hwnd(app) else {
        tracing::warn!("main window handle not available, shutdown hook not installed");
        return;
    };
    unsafe {
        let ok = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
        tracing::info!("Windows shutdown hook installed: {}", ok.as_bool());
    }
}

pub fn clear_block_reason(app: &AppHandle) {
    if let Some(hwnd) = main_hwnd(app) {
        unsafe {
            let _ = ShutdownBlockReasonDestroy(hwnd);
        }
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_QUERYENDSESSION {
        if let Some((app, state)) = CTX.get() {
            state.write_heartbeat(Utc::now());
            if state.running_count() > 0 && !state.exit_confirmed() {
                let reason: Vec<u16> = "Ein Timer läuft noch – bitte in YaTTT entscheiden.\0"
                    .encode_utf16()
                    .collect();
                let _ = ShutdownBlockReasonCreate(hwnd, PCWSTR(reason.as_ptr()));
                state.shutdown_prompt_open.store(true, Ordering::SeqCst);
                crate::tray::show_main_window(app);
                crate::jobs::emit(app, "exit-requested", "shutdown");
                return LRESULT(0);
            }
        }
        return LRESULT(1);
    }
    if msg == WM_ENDSESSION {
        if let Some((_, state)) = CTX.get() {
            state.write_heartbeat(Utc::now());
            if wparam.0 != 0 && state.running_count() > 0 && !state.exit_confirmed() {
                let _ = state.store.set_meta("kept_running_on_exit", "1");
            }
        }
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}
