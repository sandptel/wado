//! The app icon's shortcuts (long-press: Trackpad, Files, Shell, Start / join session), and the
//! Virtual input links (`?go=keyboard`, `?go=gamepad`, `?go=input`; `?go=trackpad` is unchanged): each opens
//! wado at `?go=…`, and this follows it on the **last used computer** — the selected profile.
//!
//! It goes there the way a person would: that computer's page opens at once, and when the
//! computer has answered, the shortcut's door opens — the trackpad, the file manager, a shell.
//! *Start / join session* opens nothing by itself: it lands on the computer's session door (its
//! Desktop tile), where starting, rejoining or taking over is the person's choice — a computer
//! can run several sessions for several devices, and a shortcut must not pick one for them.
//!
//! If there is no such computer, or it is offline or does not know this phone yet, or it says
//! nothing for [`TIMEOUT_MS`], wado falls back to home with a note saying why — never a spinner
//! left running. A door this phone may not use stays on the computer's page, which says so.
//!
//! `js/deeplink.js` takes `?go=` out of the address; [`decide`] is the whole policy, pure, so
//! each shortcut and each fallback is a unit test. The URLs are the manifest's, and stay stable.

use dioxus::prelude::*;

use crate::{
    bridge, reach,
    route::{self, View},
    state::Ui,
    ui::computer::{actions, input::open as input},
};

/// How long a shortcut waits for its computer before giving up and showing home.
pub const TIMEOUT_MS: u32 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    Trackpad,
    /// The touchpad screen with the keyboard up.
    Keyboard,
    /// The gamepad, full screen, with no session.
    Gamepad,
    /// The Virtual input hub.
    Input,
    Files,
    Shell,
    Session,
    /// A Claude notification: its computer's page, at its Claude cards.
    Claude,
}

impl Go {
    pub fn parse(s: &str) -> Option<Go> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trackpad" | "touchpad" => Some(Go::Trackpad),
            "keyboard" => Some(Go::Keyboard),
            "gamepad" => Some(Go::Gamepad),
            "input" => Some(Go::Input),
            "files" => Some(Go::Files),
            "shell" => Some(Go::Shell),
            "session" | "desktop" => Some(Go::Session),
            "claude" => Some(Go::Claude),
            _ => None,
        }
    }

    fn what(self) -> &'static str {
        match self {
            Go::Trackpad => "the touchpad",
            Go::Keyboard => "the keyboard",
            Go::Gamepad => "the gamepad",
            Go::Input => "its virtual input",
            Go::Files => "its files",
            Go::Shell => "a shell",
            Go::Session => "the desktop",
            Go::Claude => "Claude",
        }
    }
}

/// What is known about the last used computer, as far as a shortcut cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// A computer has been used from this phone (a profile is selected).
    pub known: bool,
    /// It answers.
    pub online: bool,
    /// It is known not to: no daemon at its Remote ID, or no network on this phone. A relay not
    /// reached *yet* is not this — the first dial looks the same for a second or two.
    pub offline: bool,
    /// The computer does not know this phone yet, or is still confirming it.
    pub unpaired: bool,
    /// It has said enough to act on: its grants.
    pub ready: bool,
    /// This phone may use the door.
    pub allowed: bool,
    pub timed_out: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    NoComputer,
    Offline,
    Unpaired,
    NoAnswer,
    NotAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Not yet: the computer has not answered.
    Wait,
    /// Open the door (for a session: show its door).
    Act,
    /// Fall back to home, saying why.
    Home(Why),
    /// Stay on the computer's page, saying why — it shows the door shut.
    Stay(Why),
}

pub fn decide(go: Go, s: &Seen) -> Next {
    if !s.known {
        return Next::Home(Why::NoComputer);
    }
    if s.unpaired {
        return Next::Home(Why::Unpaired);
    }
    if s.offline {
        return Next::Home(Why::Offline);
    }
    if s.online && (s.ready || go == Go::Session) {
        if !s.allowed {
            return Next::Stay(Why::NotAllowed);
        }
        return Next::Act;
    }
    if s.timed_out {
        return Next::Home(Why::NoAnswer);
    }
    Next::Wait
}

fn words(why: Why, go: Go, name: &str) -> String {
    match why {
        Why::NoComputer => "Add a computer first — then this shortcut opens it.".into(),
        Why::Offline => format!("{name} is offline — here are your computers."),
        Why::Unpaired => format!("{name} hasn't paired with this phone yet."),
        Why::NoAnswer => format!("{name} didn't answer — here are your computers."),
        Why::NotAllowed => format!("{name} doesn't allow {} from this phone.", go.what()),
    }
}

/// The session door on the computer's page, brought into view and focused.
const SHOW_SESSION_DOOR: &str = "(() => { const d = document.querySelector('#computer .act.primary'); \
     if (d) { d.scrollIntoView({ block: 'center', behavior: 'smooth' }); d.focus({ preventScroll: true }); } })();";

/// The Claude cards on the computer's page, brought into view.
const SHOW_CLAUDE: &str = "(() => { const d = document.getElementById('claude'); \
     if (d) d.scrollIntoView({ block: 'start', behavior: 'smooth' }); })();";

/// Take the shortcut once the saved settings are in, and follow it.
pub fn follow(ui: Ui) {
    let mut taken = use_signal(|| false);
    let mut timed_out = use_signal(|| false);

    use_effect(move || {
        if !(ui.live.loaded)() || *taken.peek() {
            return;
        }
        taken.set(true);
        spawn(async move {
            let Ok(v) = document::eval("return window.__wado.takeGo();").await else {
                return;
            };
            let Some(go) = v.as_str().and_then(Go::parse) else {
                return;
            };
            let mut live = ui.live;
            live.go.set(Some(go));
            let key = crate::profile::current_key(ui);
            if route::index_of(ui, &key).is_some() {
                route::open_computer(ui, &key);
            }
            bridge::sleep(TIMEOUT_MS).await;
            timed_out.set(true);
        });
    });

    use_effect(move || {
        let Some(go) = (ui.live.go)() else {
            return;
        };
        let mut live = ui.live;
        // A session came up on its own (a crash-resume): that is where the person is now.
        if (live.session_on)() {
            live.go.set(None);
            return;
        }
        let key = crate::profile::current_key(ui);
        let known = route::index_of(ui, &key).is_some();
        let r = reach::of(ui, &key);
        let link = (live.link)();
        let d = actions::doors(&r);
        let relay = (ui.set.conn_mode)() == "relay";
        let seen = Seen {
            known,
            online: r.online,
            offline: r.tone == "bad" && !(link.state == "offline" && link.why != "no_network"),
            unpaired: !(live.unpaired)().is_empty() || !(live.pair_sas)().is_empty(),
            ready: r.config.is_some() || r.host.is_some(),
            allowed: match go {
                Go::Trackpad | Go::Keyboard => relay && d.pad,
                Go::Gamepad => relay && d.pad && !crate::ui::computer::input::open::too_old(&r),
                Go::Input => true,
                Go::Files => relay && d.files,
                Go::Shell => relay && d.shell,
                Go::Session => true,
                Go::Claude => relay && r.may("shells"),
            },
            timed_out: timed_out(),
        };
        let name = crate::profile::current_name(ui);
        match decide(go, &seen) {
            Next::Wait => {}
            Next::Act => {
                live.go.set(None);
                match go {
                    Go::Trackpad => input::open(ui, input::Door::Touchpad),
                    Go::Keyboard => input::open(ui, input::Door::Keyboard),
                    Go::Gamepad => input::open(ui, input::Door::Gamepad),
                    Go::Input => input::hub(ui),
                    Go::Files => bridge::call("window.__wado.filesOpen();".to_string()),
                    Go::Shell => crate::ui::console::open_shells(ui),
                    Go::Session => bridge::call(SHOW_SESSION_DOOR.to_string()),
                    Go::Claude => bridge::call(SHOW_CLAUDE.to_string()),
                }
            }
            Next::Home(why) => {
                live.go.set(None);
                live.note.set(words(why, go, &name));
                if matches!(*live.view.peek(), View::Computer(_)) {
                    bridge::call(format!("window.__wado.heroClose({});", bridge::js(&key)));
                } else {
                    live.view.set(View::Home);
                }
            }
            Next::Stay(why) => {
                live.go.set(None);
                live.note.set(words(why, go, &name));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Go; 4] = [Go::Trackpad, Go::Files, Go::Shell, Go::Session];

    fn up() -> Seen {
        Seen {
            known: true,
            online: true,
            ready: true,
            allowed: true,
            ..Default::default()
        }
    }

    #[test]
    fn parses_each_shortcut() {
        assert_eq!(Go::parse("trackpad"), Some(Go::Trackpad));
        assert_eq!(Go::parse("files"), Some(Go::Files));
        assert_eq!(Go::parse("shell"), Some(Go::Shell));
        assert_eq!(Go::parse("session"), Some(Go::Session));
        assert_eq!(Go::parse(" Desktop "), Some(Go::Session));
        assert_eq!(Go::parse(""), None);
        assert_eq!(Go::parse("home"), None);
        assert_eq!(Go::parse("javascript:alert(1)"), None);
    }

    #[test]
    fn each_shortcut_opens_its_door_on_a_computer_that_answered() {
        for go in ALL {
            assert_eq!(decide(go, &up()), Next::Act, "{go:?}");
        }
    }

    #[test]
    fn waits_while_the_computer_has_not_answered() {
        for go in ALL {
            let s = Seen {
                online: false,
                ready: false,
                ..up()
            };
            assert_eq!(decide(go, &s), Next::Wait, "{go:?}");
        }
        // A door needs the grants; the session door only needs the computer there.
        for go in [Go::Trackpad, Go::Files, Go::Shell] {
            let s = Seen {
                ready: false,
                ..up()
            };
            assert_eq!(decide(go, &s), Next::Wait, "{go:?} online, grants not in");
        }
        let s = Seen {
            ready: false,
            ..up()
        };
        assert_eq!(decide(Go::Session, &s), Next::Act);
    }

    #[test]
    fn falls_back_home_when_unknown_offline_unpaired_or_silent() {
        for go in ALL {
            let none = Seen::default();
            assert_eq!(decide(go, &none), Next::Home(Why::NoComputer));
            let off = Seen {
                online: false,
                offline: true,
                ..up()
            };
            assert_eq!(decide(go, &off), Next::Home(Why::Offline));
            let unpaired = Seen {
                unpaired: true,
                ..up()
            };
            assert_eq!(decide(go, &unpaired), Next::Home(Why::Unpaired));
            let silent = Seen {
                online: false,
                ready: false,
                timed_out: true,
                ..up()
            };
            assert_eq!(decide(go, &silent), Next::Home(Why::NoAnswer));
        }
    }

    #[test]
    fn a_shut_door_stays_on_the_computer_page() {
        for go in [Go::Trackpad, Go::Files, Go::Shell] {
            let s = Seen {
                allowed: false,
                ..up()
            };
            assert_eq!(decide(go, &s), Next::Stay(Why::NotAllowed));
        }
    }

    #[test]
    fn an_answer_after_the_timeout_still_counts() {
        let s = Seen {
            timed_out: true,
            ..up()
        };
        assert_eq!(decide(Go::Files, &s), Next::Act);
    }
}
