//! The three doors and the hub, one place: the Computer page's tile, the hub's cards and their
//! Open buttons, the settings pages' head, the home widget and `?go=` all open them through here.
//!
//! All three need the `input` grant and the passkey step-up (D3). The gamepad away from a session
//! also needs a daemon new enough to carry it (`ConfigState.caps` holds `pad-gamepad`, §5.9).

use crate::{bridge, reach::Reach, route, state::Ui, ui::pages::Page};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    Touchpad,
    Keyboard,
    Gamepad,
}

/// Why a door is shut, in plain words; `None` when it opens.
pub fn shut(ui: Ui, r: &Reach, door: Door) -> Option<String> {
    let relay = (ui.set.conn_mode)() == "relay";
    if !r.online {
        return Some("Offline".into());
    }
    if !relay {
        return Some("Needs the relay link".into());
    }
    if !r.may("input") {
        return Some("Not allowed for this phone".into());
    }
    if door == Door::Gamepad && too_old(r) {
        return Some("Update wado on the computer".into());
    }
    None
}

/// The daemon has answered and cannot carry the gamepad away from a session.
pub fn too_old(r: &Reach) -> bool {
    r.config
        .as_ref()
        .is_some_and(|c| !c.caps.iter().any(|x| x == "pad-gamepad"))
}

pub fn open(ui: Ui, door: Door) {
    let name = crate::profile::current_name(ui);
    match door {
        Door::Touchpad => bridge::call(
            "window.__wado.padOpenKeyboard = undefined; window.__wado.padOpen();".into(),
        ),
        Door::Keyboard => bridge::call(
            "window.__wado.padOpenKeyboard = true; window.__wado.padOpen({keyboard: true});".into(),
        ),
        Door::Gamepad => bridge::call(format!(
            "window.__wado.gpadOpen({{where: 'away', name: {}}});",
            bridge::js(&name)
        )),
    }
}

/// The gamepad straight into its editor, or its comfort check (from the settings page).
pub fn edit(ui: Ui, state: &str) {
    let name = crate::profile::current_name(ui);
    let where_ = if (ui.live.session_on)() {
        "session"
    } else {
        "away"
    };
    bridge::call(format!(
        "window.__wado.gpadOpen({{where: {}, state: {}, name: {}}});",
        bridge::js(&where_),
        bridge::js(&state),
        bridge::js(&name)
    ));
}

/// Run one of the computer's quick actions (`Run` → `Ran`): on the selected computer's link, or
/// through another computer's side link. Every Virtual input chip goes through here, so the shared
/// pending-action feedback can wrap this one call.
pub fn run_action(selected: bool, pk: &str, id: &str) {
    bridge::call(if selected {
        format!("window.__wado.actionRun({});", bridge::js(&id))
    } else {
        format!(
            "window.__wado.sideDo({}, {{type: 'pad_in', msg: {{k: 'run', id: {}}}}});",
            bridge::js(&pk),
            bridge::js(&id)
        )
    });
}

pub fn hub(ui: Ui) {
    route::go(ui, Page::Input);
}

pub fn settings(ui: Ui, door: Door) {
    route::go(
        ui,
        match door {
            Door::Touchpad => Page::InputTouchpad,
            Door::Keyboard => Page::InputKeyboard,
            Door::Gamepad => Page::InputGamepad,
        },
    );
}
