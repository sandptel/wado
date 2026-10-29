//! Starting Claude, and reaching it again: a new session in a project folder, and a shell on a
//! session that is already running.
//!
//! With tmux (decided `2026-10-27`): `tmux new-session -d -s wado-<project> claude` in the folder,
//! then a wado shell running `tmux attach-session -t =wado-<project>` — the session survives the
//! daemon and can be attached at the desk. Without tmux: a wado shell running `claude` itself.
//! Every command is an argv. The folder is never put in a command line at all — it is the
//! working directory tmux (or the shell) starts in — so no folder name can be read as code,
//! not even by tmux's own format expansion.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The tmux session name for a project folder: `wado-` and the folder's name, reduced to what
/// tmux takes in a target (letters, digits, `-`, `_`), then `-2`, `-3`… while `taken`.
pub fn tmux_name(dir: &Path, taken: impl Fn(&str) -> bool) -> String {
    let base: String = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base: String = base.chars().take(40).collect();
    let base = format!("wado-{}", if base.is_empty() { "claude" } else { &base });
    let mut name = base.clone();
    let mut n = 2;
    while taken(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// Start Claude detached in a new tmux session. Run in the project folder.
pub fn new_argv(name: &str) -> Vec<String> {
    ["tmux", "new-session", "-d", "-s", name, "claude"]
        .map(String::from)
        .to_vec()
}

/// A terminal on that session. `=`: this exact session, never a prefix match.
pub fn attach_argv(name: &str) -> Vec<String> {
    ["tmux", "attach-session", "-t", &format!("={name}")]
        .map(String::from)
        .to_vec()
}

/// Is `prog` on the PATH?
pub fn on_path(prog: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| {
            let f = d.join(prog);
            f.is_file()
                && std::fs::metadata(&f)
                    .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
                    .unwrap_or(false)
        })
    })
}

pub fn has_tmux() -> bool {
    on_path("tmux")
}

fn tmux_has(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", &format!("={name}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The folder to start in, checked: it must exist and be a folder.
pub fn folder(dir: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(dir);
    if !p.is_absolute() {
        return Err("pick a folder on the computer".into());
    }
    let p = p
        .canonicalize()
        .map_err(|_| format!("{dir} isn't there any more"))?;
    if !p.is_dir() {
        return Err(format!("{dir} is not a folder"));
    }
    Ok(p)
}

/// Start Claude in `dir`, and open a shell on it. The shell's id, and the name the session's
/// hooks will report (its tmux name, or the `WADO_CLAUDE` label).
pub fn start(dir: &str, cols: u16, rows: u16) -> Result<(u32, String), String> {
    let dir = folder(dir)?;
    if !on_path("claude") {
        return Err(
            "Claude Code isn't installed on this computer (no `claude` on its PATH)".into(),
        );
    }
    let project = dir
        .file_name()
        .map_or("claude".into(), |n| n.to_string_lossy().into_owned());
    if has_tmux() {
        let name = tmux_name(&dir, tmux_has);
        let ok = Command::new("tmux")
            .args(&new_argv(&name)[1..])
            .current_dir(&dir)
            .status()
            .map_err(|e| format!("tmux would not start: {e}"))?;
        if !ok.success() {
            return Err("tmux could not start the session".into());
        }
        let id = crate::shells::open_cmd(
            cols,
            rows,
            &attach_argv(&name),
            &dir,
            &[],
            format!("claude · {project}"),
        )?;
        Ok((id, name))
    } else {
        let label = tmux_name(&dir, |_| false);
        let id = crate::shells::open_cmd(
            cols,
            rows,
            &["claude".to_string()],
            &dir,
            &[("WADO_CLAUDE".into(), label.clone())],
            format!("claude · {project}"),
        )?;
        Ok((id, label))
    }
}

/// A shell on the running session known as `tmux`: attach to it.
pub fn attach(tmux: &str, cwd: &str, cols: u16, rows: u16) -> Result<u32, String> {
    if !tmux.starts_with("wado-") || !has_tmux() || !tmux_has(tmux) {
        return Err("that Claude session is no longer running in tmux".into());
    }
    let dir =
        folder(cwd).unwrap_or_else(|_| std::env::var_os("HOME").map_or("/".into(), PathBuf::from));
    let project = tmux.trim_start_matches("wado-").to_string();
    crate::shells::open_cmd(
        cols,
        rows,
        &attach_argv(tmux),
        &dir,
        &[],
        format!("claude · {project}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_tmux_safe_and_unique() {
        assert_eq!(
            tmux_name(Path::new("/home/me/My API"), |_| false),
            "wado-my-api"
        );
        assert_eq!(
            tmux_name(Path::new("/x/a.b:c;rm -rf ~"), |_| false),
            "wado-a-b-c-rm-rf"
        );
        assert_eq!(tmux_name(Path::new("/"), |_| false), "wado-claude");
        let taken = ["wado-api", "wado-api-2"];
        assert_eq!(
            tmux_name(Path::new("/w/api"), |n| taken.contains(&n)),
            "wado-api-3"
        );
    }

    #[test]
    fn commands_are_argv_and_never_carry_the_folder() {
        let dir = Path::new("/home/me/$(touch pwned); `id` \"quoted\" #(date)");
        let name = tmux_name(dir, |_| false);
        let new = new_argv(&name);
        assert_eq!(new[..5], ["tmux", "new-session", "-d", "-s", name.as_str()]);
        assert_eq!(new.last().unwrap(), "claude");
        assert!(
            new.iter()
                .all(|a| !a.contains('$') && !a.contains('`') && !a.contains('#'))
        );
        assert_eq!(
            attach_argv(&name),
            ["tmux", "attach-session", "-t", &format!("={name}")]
        );
    }

    #[test]
    fn only_real_folders() {
        assert!(folder("relative/path").is_err());
        assert!(folder("/definitely/not/here/wado").is_err());
        assert!(folder("/").is_ok());
    }
}
