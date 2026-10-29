//! Screenshots ([`Screenshot`] on Linux): `grim` on the desktop's display, with `slurp` to choose
//! an area.

use std::path::PathBuf;
use std::time::Duration;

use wado_platform::desktop::Screenshot;
use wado_platform::{Fut, Res};

use super::on_desktop::desktop;
use super::probe::tool;

/// Choosing an area with `slurp` is done by hand on the laptop's screen.
const CHOOSE: Duration = Duration::from_secs(60);
const CAPTURE: Duration = Duration::from_secs(10);

/// `grim`, and `slurp` when installed.
pub struct Grim {
    region: Result<(), String>,
}

impl Grim {
    /// Offered when `grim` is installed, inside a Wayland desktop.
    pub fn probe() -> Result<Grim, String> {
        tool("grim")?;
        super::on_desktop::display()?;
        Ok(Grim {
            region: tool("slurp"),
        })
    }
}

impl Screenshot for Grim {
    fn can_region(&self) -> Res<()> {
        self.region.clone()
    }

    fn shoot(&self, region: bool, to: PathBuf) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let file = to.to_string_lossy();
            if region {
                self.can_region()?;
                let area = desktop("slurp", &[], CHOOSE)
                    .await
                    .map_err(|_| "no area was chosen".to_string())?;
                desktop("grim", &["-g", &area, &file], CAPTURE).await?;
            } else {
                desktop("grim", &[&file], CAPTURE).await?;
            }
            Ok(())
        })
    }
}
