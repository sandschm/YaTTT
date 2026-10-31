//! macOS: implement `applicationShouldTerminate:` on tao's app delegate at runtime.
//! `class_replaceMethod` adds the method when it is missing and replaces it otherwise, so this
//! keeps working if a future tao version implements the selector itself.

use super::{apply_choice, ShutdownChoice};
use crate::state::AppState;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{NSAlert, NSApplication};
use objc2_foundation::{MainThreadMarker, NSString};
use std::ffi::CString;
use std::sync::{Arc, OnceLock};
use tauri::AppHandle;

static CTX: OnceLock<(AppHandle, Arc<AppState>)> = OnceLock::new();

const NS_TERMINATE_CANCEL: usize = 0;
const NS_TERMINATE_NOW: usize = 1;
const NS_ALERT_FIRST_BUTTON: isize = 1000;
const NS_ALERT_SECOND_BUTTON: isize = 1001;

pub fn install(app: AppHandle, state: Arc<AppState>) {
    let _ = CTX.set((app, state));
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("shutdown hook must be installed on the main thread");
        return;
    };
    let nsapp = NSApplication::sharedApplication(mtm);
    let Some(delegate) = nsapp.delegate() else {
        tracing::warn!("no NSApplication delegate, shutdown hook not installed");
        return;
    };
    unsafe {
        let cls: &AnyClass = msg_send![&*delegate, class];
        let types = CString::new("L@:@").unwrap();
        let imp: unsafe extern "C-unwind" fn() = std::mem::transmute(
            application_should_terminate
                as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
        );
        let previous = objc2::ffi::class_replaceMethod(
            cls as *const AnyClass as *mut AnyClass,
            sel!(applicationShouldTerminate:),
            imp,
            types.as_ptr(),
        );
        tracing::info!(
            "macOS shutdown hook installed (replaced existing: {})",
            previous.is_some()
        );
    }
}

unsafe extern "C-unwind" fn application_should_terminate(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> usize {
    let Some((_app, state)) = CTX.get() else {
        return NS_TERMINATE_NOW;
    };
    if state.exit_confirmed() || state.running_count() == 0 {
        state.write_heartbeat(chrono::Utc::now());
        return NS_TERMINATE_NOW;
    }
    let choice = ask(state);
    if apply_choice(state, choice) {
        NS_TERMINATE_NOW
    } else {
        NS_TERMINATE_CANCEL
    }
}

fn ask(state: &AppState) -> ShutdownChoice {
    // AppKit calls the delegate on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let running = state.store.running_entries().unwrap_or_default();
    let keys: Vec<String> = running.iter().map(|e| e.issue_key.clone()).collect();
    {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(yattt_core::tr!(
            "Ein Timer läuft noch",
            "A timer is still running"
        )));
        alert.setInformativeText(&NSString::from_str(&yattt_core::tr!(
            "YaTTT erfasst gerade Zeit für {keys}. Soll der Timer gestoppt werden oder weiterlaufen? Bei „Weiterlaufen lassen“ zählt die Zeit weiter und wird beim nächsten Start angezeigt.",
            "YaTTT is tracking time for {keys}. Stop the timer or keep it running? With \"Keep running\" the time keeps counting and is shown at the next start.",
            keys = keys.join(", ")
        )));
        alert.addButtonWithTitle(&NSString::from_str(yattt_core::tr!(
            "Timer stoppen und beenden",
            "Stop timer and quit"
        )));
        alert.addButtonWithTitle(&NSString::from_str(yattt_core::tr!(
            "Weiterlaufen lassen und beenden",
            "Keep running and quit"
        )));
        alert.addButtonWithTitle(&NSString::from_str(yattt_core::tr!("Abbrechen", "Cancel")));
        let response = alert.runModal();
        match response {
            NS_ALERT_FIRST_BUTTON => ShutdownChoice::StopAndQuit,
            NS_ALERT_SECOND_BUTTON => ShutdownChoice::KeepAndQuit,
            _ => ShutdownChoice::Cancel,
        }
    }
}
