//! Opening an app on the desktop: a terminal, the browser, the file manager. Started and left
//! to run — never awaited, so the tap is answered as soon as the app is on its way.

use super::detect::Found;
use super::{Action, Does, Need};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Terminal,
    Browser,
    Files,
}

pub fn all() -> Vec<Action> {
    let open = vec![Need::Tool("xdg-open"), Need::Desktop];
    vec![
        Action::new(
            "open-terminal",
            "Terminal",
            "Apps",
            "term",
            vec![Need::Terminal, Need::Desktop],
            Does::App(Op::Terminal),
        ),
        Action::new(
            "open-browser",
            "Browser",
            "Apps",
            "net",
            open.clone(),
            Does::App(Op::Browser),
        ),
        Action::new(
            "open-files",
            "Files",
            "Apps",
            "folder",
            open,
            Does::App(Op::Files),
        ),
    ]
}

pub fn run(op: Op, found: &Found) -> Result<(), String> {
    let detached = super::proc::detached;
    match op {
        Op::Terminal => {
            let term = found
                .terminal
                .as_deref()
                .ok_or_else(|| Need::Terminal.why())?;
            detached(term, &[])
        }
        Op::Browser => detached("xdg-open", &["https://"]),
        Op::Files => {
            let home = std::env::var("HOME").map_err(|_| "no home folder to open")?;
            detached("xdg-open", &[&home])
        }
    }
}
