//! Browser-based login: opens the Jira login page in a webview window, waits until the
//! session cookies work against `/rest/api/2/myself`, then stores them.
//!
//! This path handles SSO redirects and second factors because the real login UI is shown.

use crate::jobs::{emit, install_client};
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use yattt_core::jira::JiraClient;
use yattt_core::models::*;

pub const LOGIN_WINDOW: &str = "login";

/// Identity providers reject logins from "embedded" browsers. A regular desktop UA avoids that.
#[cfg(target_os = "macos")]
const DESKTOP_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
#[cfg(target_os = "windows")]
const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
const DESKTOP_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

fn to_stored(c: &tauri::webview::Cookie<'_>) -> StoredCookie {
    StoredCookie {
        name: c.name().to_string(),
        value: c.value().to_string(),
        domain: c.domain().map(str::to_string),
        path: c.path().map(str::to_string),
        expires: c
            .expires()
            .and_then(|e| e.datetime())
            .map(|d| d.unix_timestamp()),
        secure: c.secure().unwrap_or(false),
        http_only: c.http_only().unwrap_or(false),
    }
}

/// Opens the login window and resolves once the session works or the window is closed.
pub async fn browser_login(app: AppHandle, state: Arc<AppState>) -> Result<AuthStatus, String> {
    if state.login_in_progress.swap(true, Ordering::SeqCst) {
        return Err("Eine Anmeldung läuft bereits".into());
    }
    let result = browser_login_inner(&app, &state).await;
    state.login_in_progress.store(false, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.close();
    }
    result
}

async fn browser_login_inner(app: &AppHandle, state: &Arc<AppState>) -> Result<AuthStatus, String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if base.is_empty() {
        return Err("Bitte zuerst die Jira-URL eintragen".into());
    }
    let base_url = Url::parse(&base).map_err(|e| format!("Ungültige Jira-URL: {e}"))?;
    let login_url = Url::parse(&format!(
        "{base}/login.jsp?os_destination=%2Fsecure%2FDashboard.jspa"
    ))
    .map_err(|e| e.to_string())?;

    if let Some(existing) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = existing.close();
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let window = WebviewWindowBuilder::new(app, LOGIN_WINDOW, WebviewUrl::External(login_url))
        .title("Bei Jira anmelden")
        .inner_size(560.0, 760.0)
        .min_inner_size(420.0, 520.0)
        .center()
        .user_agent(DESKTOP_UA)
        .build()
        .map_err(|e| format!("Login-Fenster konnte nicht geöffnet werden: {e}"))?;
    let _ = window.set_focus();
    emit(app, "login-window", "opened");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15 * 60);
    let mut last_probe: Option<String> = None;
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        if app.get_webview_window(LOGIN_WINDOW).is_none() {
            emit(app, "login-window", "closed");
            return Err("Anmeldung abgebrochen".into());
        }
        if tokio::time::Instant::now() > deadline {
            return Err("Zeitüberschreitung bei der Anmeldung".into());
        }
        let cookies = match window.cookies_for_url(base_url.clone()) {
            Ok(c) => c.iter().map(to_stored).collect::<Vec<_>>(),
            Err(e) => {
                tracing::debug!("cookies_for_url failed: {e}");
                continue;
            }
        };
        if cookies.is_empty() {
            continue;
        }
        tracing::debug!(
            "login window cookies for {}: {:?}",
            base_url.host_str().unwrap_or(""),
            cookies.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
        );
        // Only probe when the cookie set changed; avoids hammering Jira while the user types.
        let fingerprint = cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join(";");
        if last_probe.as_deref() == Some(fingerprint.as_str()) {
            continue;
        }
        last_probe = Some(fingerprint);
        let probe = match JiraClient::new(
            &base,
            Credential::Cookies {
                cookies: cookies.clone(),
            },
            Deployment::Unknown,
        ) {
            Ok(c) => c,
            Err(e) => return Err(e.to_string()),
        };
        match probe.myself().await {
            Ok(user) => {
                tracing::info!("browser login succeeded for {}", user.display_name);
                let status = install_client(app, state, Credential::Cookies { cookies }).await?;
                return Ok(status);
            }
            Err(yattt_core::Error::AuthExpired(_)) => continue,
            Err(e) => {
                tracing::debug!("probe failed: {e}");
                continue;
            }
        }
    }
}

/// Forget the session: credential, client state and the webview's browsing data.
pub fn logout(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let settings = state.settings();
    let base = settings.jira_base();
    if !base.is_empty() {
        yattt_core::secrets::delete_credential(state.secrets.as_ref(), &base)
            .map_err(|e| e.to_string())?;
    }
    state.clear_session(None);
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.clear_all_browsing_data();
        let _ = w.close();
    } else {
        // Open a hidden window so that the shared webview profile can be cleared.
        if let Ok(w) = WebviewWindowBuilder::new(
            app,
            LOGIN_WINDOW,
            WebviewUrl::External(Url::parse("about:blank").unwrap()),
        )
        .visible(false)
        .build()
        {
            let _ = w.clear_all_browsing_data();
            let _ = w.close();
        }
    }
    emit(app, "auth-changed", state.auth_status());
    Ok(())
}
