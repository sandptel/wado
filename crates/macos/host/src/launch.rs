//! Opening Terminal, the default browser or Finder on the Mac, and screenshots.

use std::path::PathBuf;
use std::time::Duration;

use wado_platform::desktop::{App, Launcher, Screenshot};
use wado_platform::{Fut, Res};

pub struct MacLauncher;

impl Launcher for MacLauncher {
    fn can(&self, _app: App) -> Res<()> {
        Ok(())
    }

    fn open(&self, app: App) -> Res<()> {
        match app {
            App::Terminal => crate::cmd::detached("/usr/bin/open", &["-a", "Terminal"]),
            App::Browser => match default_browser() {
                Some(path) => crate::cmd::detached("/usr/bin/open", &["-a", &path]),
                None => Err("this Mac has no default browser".into()),
            },
            App::Files => {
                let home = std::env::var("HOME").map_err(|_| "no home folder to open")?;
                crate::cmd::detached("/usr/bin/open", &[&home])
            }
        }
    }
}

/// The app that opens `https:` links here.
fn default_browser() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};
    let url = NSURL::URLWithString(&NSString::from_str("https://example.com"))?;
    let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url)?;
    Some(app.path()?.to_string())
}

/// `screencapture`, which needs Screen Recording for anything but the desktop wallpaper.
pub struct MacScreenshot;

impl Screenshot for MacScreenshot {
    fn can_region(&self) -> Res<()> {
        Ok(())
    }

    fn shoot(&self, region: bool, to: PathBuf) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let file = to.to_string_lossy().into_owned();
            let mut args = vec!["-x"];
            if region {
                args.push("-i");
            }
            args.push(&file);
            let limit = if region { 60 } else { 10 };
            crate::cmd::run("/usr/sbin/screencapture", &args, Duration::from_secs(limit)).await?;
            if !to.exists() {
                return Err("no area was chosen".into());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_a_default_browser() {
        assert!(super::default_browser().is_some_and(|p| p.ends_with(".app")));
    }
}
