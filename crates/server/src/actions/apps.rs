//! Opening an app on the desktop through the platform's launcher: a terminal, the browser, the
//! file manager.

use wado_platform::desktop::App;

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    [
        ("open-terminal", "Terminal", "term", App::Terminal),
        ("open-browser", "Browser", "net", App::Browser),
        ("open-files", "Files", "folder", App::Files),
    ]
    .into_iter()
    .map(|(id, name, icon, app)| {
        Action::new(
            id,
            name,
            "Apps",
            icon,
            vec![Need::Launch(app)],
            Does::App(app),
        )
    })
    .collect()
}
