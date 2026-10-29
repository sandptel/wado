// SPDX-License-Identifier: AGPL-3.0-only
//! From a `watch` channel into the event loop. Each subscription is one thread awaiting
//! `changed()`; every new value is handed to the UI thread with `upgrade_in_event_loop`, so the
//! UI only ever runs short, synchronous applies and never waits on a producer.
//!
//! `apply` is a plain function: the UI-side state it touches lives in thread-locals on the UI
//! thread (`host::with`, `client::with`), so nothing non-`Send` crosses.

use slint::Weak;
use tokio::sync::watch;

use crate::AppWindow;

pub fn subscribe<T>(mut rx: watch::Receiver<T>, weak: Weak<AppWindow>, apply: fn(&AppWindow, T))
where
    T: Clone + Send + Sync + 'static,
{
    std::thread::Builder::new()
        .name("wado-desktop-bridge".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().build() else {
                return;
            };
            rt.block_on(async move {
                loop {
                    let v = rx.borrow_and_update().clone();
                    if weak.upgrade_in_event_loop(move |ui| apply(&ui, v)).is_err() {
                        return;
                    }
                    if rx.changed().await.is_err() {
                        return;
                    }
                }
            });
        })
        .expect("a thread");
}
