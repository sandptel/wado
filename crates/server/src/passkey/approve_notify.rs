//! The desktop's own notification for an approval request (`super::approve`): so the person at
//! the computer sees that a phone is waiting, without a terminal open. Best effort — the
//! desktop's `org.freedesktop.Notifications` on the session bus, if one answers; the request
//! stands either way, in `wado passkey approve` and `wado tui`.
//!
//! `WADO_NO_DESKTOP_NOTIFY=1` keeps it quiet (the e2e scripts, which run on a real desktop).

use std::collections::HashMap;
use std::time::Duration;

/// Tell the desktop that `device` asks to be approved. Returns at once.
pub fn show(device: &str) {
    if cfg!(test) || std::env::var_os("WADO_NO_DESKTOP_NOTIFY").is_some_and(|v| v != "0") {
        return;
    }
    let body = format!(
        "{device} asks to unlock without its passkey. If it is yours, run `wado passkey approve` \
         or answer in `wado tui`. Nobody answering counts as no."
    );
    tokio::spawn(async move {
        let sent = tokio::time::timeout(Duration::from_secs(3), send(&body)).await;
        if !matches!(sent, Ok(Ok(()))) {
            tracing::debug!("passkey: no desktop notification for the approval request");
        }
    });
}

async fn send(body: &str) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "wado",
            0u32,
            "dialog-password",
            "A phone asks to be let in",
            body,
            Vec::<&str>::new(),
            hints,
            // Gone when the request is: two minutes.
            120_000i32,
        ),
    )
    .await?;
    Ok(())
}
