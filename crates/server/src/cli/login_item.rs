//! `wado install` / `wado uninstall` on macOS: a LaunchAgent that starts wado at login and
//! restarts it if it stops. Optional — nothing installs it by itself.

use std::path::{Path, PathBuf};

use super::style;

pub const LABEL: &str = "io.github.sandptel.wado";

/// The agent's file.
pub fn plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

/// The agent that runs `exe daemon`, logging to `log`.
pub fn plist(exe: &Path, log: &Path) -> String {
    let (exe, log) = (exe.display(), log.display());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key><array><string>{exe}</string><string>daemon</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
    <key>ProcessType</key><string>Interactive</string>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#
    )
}

fn domain() -> String {
    // SAFETY: getuid has no preconditions.
    format!("gui/{}", unsafe { libc::getuid() })
}

pub fn install() -> i32 {
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("  {}", style::bad("cannot tell where this wado is"));
        return 1;
    };
    if !exe.to_string_lossy().contains(".app/Contents/MacOS/") {
        eprintln!(
            "  run this from Wado.app ({}), so its privacy grants are the ones that start at login",
            style::accent("Wado.app/Contents/MacOS/wado install")
        );
        return 2;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let log = Path::new(&home).join("Library/Logs/wado.log");
    let path = plist_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    if let Err(e) = std::fs::write(&path, plist(&exe, &log)) {
        eprintln!("  {}: {e}", style::bad("could not write the login item"));
        return 1;
    }
    let ok = std::process::Command::new("/bin/launchctl")
        .args(["bootstrap", &domain(), &path.to_string_lossy()])
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        println!(
            "  wado starts at login now — {}",
            style::accent("wado uninstall")
        );
        0
    } else {
        eprintln!(
            "  written to {}, but launchd did not start it",
            path.display()
        );
        1
    }
}

pub fn uninstall() -> i32 {
    let path = plist_path();
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["bootout", &format!("{}/{LABEL}", domain())])
        .status();
    match std::fs::remove_file(&path) {
        Ok(()) => println!("  wado no longer starts at login"),
        Err(_) => println!("  wado was not starting at login"),
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_runs_the_daemon_and_restarts_it() {
        let p = plist(
            Path::new("/Applications/Wado.app/Contents/MacOS/wado"),
            Path::new("/tmp/w.log"),
        );
        assert!(p.contains(
            "<string>/Applications/Wado.app/Contents/MacOS/wado</string><string>daemon</string>"
        ));
        assert!(p.contains("<key>RunAtLoad</key><true/>"));
        assert!(p.contains(LABEL));
    }
}
