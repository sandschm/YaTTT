//! Seconds since the last user input. `None` when the platform does not report it
//! (e.g. Wayland without a supported compositor); the caller then relies on the
//! time-jump watchdog for sleep detection only.

pub fn idle_seconds() -> Option<u64> {
    match user_idle::UserIdle::get_time() {
        Ok(t) => Some(t.as_seconds()),
        Err(e) => {
            tracing::debug!("idle time unavailable: {e:?}");
            None
        }
    }
}
