// SPDX-License-Identifier: AGPL-3.0-only
//! Logs: the tail of the daemon's log, filtered by level and words. The filter runs here, over
//! at most `caps::logs::KEEP` lines, only when the log or the filter changed.

use std::sync::Arc;

use slint::ComponentHandle;

use super::Ctl;
use crate::{
    AppWindow, Host, LogLine,
    caps::{logs::Line, watch::HostSnapshot},
    models::sync,
};

#[derive(Default)]
pub struct State {
    /// The log generation and filter last shown.
    shown: Option<(u64, i32, String)>,
    lines: Arc<Vec<Line>>,
    generation: u64,
}

pub fn wire(ui: &AppWindow) {
    let weak = ui.as_weak();
    ui.global::<Host>().on_log_filter(move || {
        if let Some(ui) = weak.upgrade() {
            super::with(|c| refilter(&ui, c));
        }
    });
}

pub fn apply(ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    let h = ui.global::<Host>();
    h.set_log_source(
        s.log_path
            .as_deref()
            .map_or("No daemon log found yet.".into(), |p| {
                format!("Tailing {p}")
            })
            .into(),
    );
    ctl.logs.lines = s.logs.clone();
    ctl.logs.generation = s.logs_generation;
    refilter(ui, ctl);
}

fn refilter(ui: &AppWindow, ctl: &mut Ctl) {
    let h = ui.global::<Host>();
    let level = h.get_log_level();
    let query = h.get_log_query().to_lowercase();
    let key = (ctl.logs.generation, level, query.clone());
    if ctl.logs.shown.as_ref() == Some(&key) {
        return;
    }
    let rows: Vec<LogLine> = ctl
        .logs
        .lines
        .iter()
        .filter(|l| match level {
            1 => matches!(l.level.as_str(), "INFO" | "WARN" | "ERROR"),
            2 => matches!(l.level.as_str(), "WARN" | "ERROR"),
            3 => l.activity(),
            _ => true,
        })
        .filter(|l| {
            query.is_empty()
                || l.msg.to_lowercase().contains(&query)
                || l.target.to_lowercase().contains(&query)
        })
        .map(super::overview::row)
        .collect();
    sync(&ctl.models.logs, rows);
    ctl.logs.shown = Some(key);
}
