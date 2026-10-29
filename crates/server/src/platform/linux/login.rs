//! Locking and sleeping ([`SystemPower`] on Linux): `loginctl lock-session` and
//! `systemctl suspend`.

use wado_platform::power::{PowerOp, SystemPower};
use wado_platform::{Fut, Res};

use super::cmd::run;
use super::probe::tool;

/// systemd-logind, through its tools.
pub struct Logind;

impl Logind {
    /// Offered when either tool is installed; each operation says what it lacks.
    pub fn probe() -> Result<Logind, String> {
        tool("loginctl").or_else(|_| tool("systemctl"))?;
        Ok(Logind)
    }
}

fn prog(op: PowerOp) -> &'static str {
    match op {
        PowerOp::Lock => "loginctl",
        PowerOp::Suspend => "systemctl",
    }
}

impl SystemPower for Logind {
    fn can(&self, op: PowerOp) -> Res<()> {
        tool(prog(op))
    }

    fn run(&self, op: PowerOp) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let args: &[&str] = match op {
                PowerOp::Lock => &["lock-session"],
                PowerOp::Suspend => &["suspend"],
            };
            run(prog(op), args).await.map(drop)
        })
    }
}
