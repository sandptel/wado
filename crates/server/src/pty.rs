//! Shells on a pseudo-terminal, as the daemon allows them: whether shells are on at all, which
//! program runs, where it starts. The terminal itself is the platform's
//! (`wado_platform::pty`, provided on Linux by `crate::platform::linux::pty`).

use tokio::sync::mpsc;
use wado_platform::Platform;
use wado_platform::pty::{Program, PtyRequest, PtySession};

/// Shown when `shells { enabled #false }`.
pub const DISABLED: &str = "shells are disabled on this host (shells.enabled)";

/// What runs: `ssh <host>` when a host is given, else the user's own shell — `shells { program }`,
/// else `$SHELL`, else `/bin/sh`.
pub fn program(host: Option<&str>) -> Program {
    match host {
        Some(alias) => Program::Ssh(alias.to_string()),
        None => Program::Shell(
            wado_config::live::current()
                .shells
                .program
                .clone()
                .or_else(|| std::env::var("SHELL").ok())
                .unwrap_or_else(|| "/bin/sh".to_string()),
        ),
    }
}

/// Open a terminal running [`program`] in the home folder, its output to `out`.
///
/// `host` must already be checked against the host's own ssh config
/// (see [`crate::shells::hosts`]).
pub fn open(
    cols: u16,
    rows: u16,
    host: Option<&str>,
    out: mpsc::Sender<String>,
) -> Result<Box<dyn PtySession>, String> {
    open_on(crate::platform::get(), cols, rows, host, out)
}

/// Open a terminal running `argv` — a program and its arguments, never a shell string — in
/// `cwd` with `env` added: Claude's sessions (`crate::claude::spawn`).
pub fn open_cmd(
    cols: u16,
    rows: u16,
    argv: &[String],
    cwd: &std::path::Path,
    env: &[(String, String)],
    out: mpsc::Sender<String>,
) -> Result<Box<dyn PtySession>, String> {
    if !wado_config::live::current().shells.enabled {
        return Err(DISABLED.into());
    }
    if argv.is_empty() {
        return Err("nothing to run".into());
    }
    let req = PtyRequest {
        cols,
        rows,
        program: Program::Command {
            argv: argv.to_vec(),
            env: env.to_vec(),
        },
        cwd: Some(cwd.to_path_buf()),
    };
    crate::platform::get()
        .pty()
        .map_err(|a| a.why)?
        .spawn(req, out)
}

/// [`open`], on `p`'s terminals.
pub fn open_on(
    p: &Platform,
    cols: u16,
    rows: u16,
    host: Option<&str>,
    out: mpsc::Sender<String>,
) -> Result<Box<dyn PtySession>, String> {
    if !wado_config::live::current().shells.enabled {
        return Err(DISABLED.into());
    }
    let req = PtyRequest {
        cols,
        rows,
        program: program(host),
        cwd: std::env::var_os("HOME").map(Into::into),
    };
    p.pty().map_err(|a| a.why)?.spawn(req, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_runs_ssh_with_the_alias_as_one_argument() {
        assert_eq!(
            program(Some("box; rm -rf /")),
            Program::Ssh("box; rm -rf /".into())
        );
        assert!(matches!(program(None), Program::Shell(s) if !s.is_empty()));
    }

    #[tokio::test]
    async fn a_terminal_opens_through_the_platform() {
        let f = wado_platform::fake::Fakes::new();
        let (tx, mut rx) = mpsc::channel(4);
        let mut t = open_on(&f.platform(), 80, 24, Some("box"), tx).unwrap();
        t.write("ls\n").unwrap();
        assert_eq!(rx.recv().await.as_deref(), Some("ls\n"));
        assert_eq!(f.log.calls()[0], "pty.spawn Ssh(\"box\") 80x24");
    }
}
