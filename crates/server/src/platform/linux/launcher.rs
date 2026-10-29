//! Opening apps on the desktop ([`Launcher`] on Linux): a terminal, the browser, the file
//! manager. Started and left to run — never awaited, so the tap is answered as soon as the app is
//! on its way.

use wado_platform::Res;
use wado_platform::desktop::{App, Launcher};

use super::on_desktop::detached;
use super::probe::{NO_TERMINAL, terminal_here, tool};

/// `xdg-open` and the terminal, on the desktop's display.
pub struct XdgLauncher {
    /// The terminal program, found when probed.
    terminal: Option<String>,
    opener: Result<(), String>,
}

impl XdgLauncher {
    /// Offered inside a Wayland desktop; what each app needs is checked by [`Launcher::can`].
    pub fn probe() -> Result<XdgLauncher, String> {
        super::on_desktop::display()?;
        Ok(XdgLauncher {
            terminal: terminal_here(),
            opener: tool("xdg-open"),
        })
    }
}

impl Launcher for XdgLauncher {
    fn can(&self, app: App) -> Res<()> {
        match app {
            App::Terminal => self.terminal.as_ref().map(drop).ok_or(NO_TERMINAL.into()),
            App::Browser | App::Files => self.opener.clone(),
        }
    }

    fn open(&self, app: App) -> Res<()> {
        self.can(app)?;
        match app {
            App::Terminal => detached(self.terminal.as_deref().unwrap_or_default(), &[]),
            App::Browser => detached("xdg-open", &["https://"]),
            App::Files => {
                let home = std::env::var("HOME").map_err(|_| "no home folder to open")?;
                detached("xdg-open", &[&home])
            }
        }
    }
}
