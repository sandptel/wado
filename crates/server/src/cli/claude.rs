//! `wado claude hooks install|uninstall [--yes]` — put wado's hooks into Claude Code's settings,
//! or take them out. Always shows the diff first and asks; `--yes` skips the question, never the
//! diff. The file is backed up once beside itself before the first change.

use std::io::{BufRead, IsTerminal, Write};

use super::style::{accent, bad, bold, dim, good};
use crate::claude::{diff, hooks, ojson::J, settings_path};

pub fn run(args: &[String]) -> i32 {
    let yes = args.iter().any(|a| a == "--yes" || a == "-y");
    let words: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    match words.as_slice() {
        ["hooks", "install"] => change(true, yes),
        ["hooks", "uninstall"] => change(false, yes),
        ["hooks"] | ["hooks", "status"] => {
            if crate::claude::hooks_installed() {
                println!(
                    "  {} wado's hooks are in {}",
                    good("✓"),
                    settings_path().display()
                );
            } else {
                println!(
                    "  {} not installed — {}",
                    dim("○"),
                    accent("wado claude hooks install")
                );
            }
            0
        }
        _ => {
            eprintln!("  usage: wado claude hooks install|uninstall [--yes]");
            2
        }
    }
}

fn change(install: bool, yes: bool) -> i32 {
    let path = settings_path();
    let old_text = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}\n".into());
    let old = match J::parse(&old_text) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "  {} {} is not valid JSON ({e}) — fix it first; wado will not rewrite a file it cannot read",
                bad("✗"),
                path.display()
            );
            return 1;
        }
    };
    let new = if install {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "wado".into());
        hooks::install(&old, &hooks::command(&exe))
    } else {
        hooks::uninstall(&old)
    };
    if new == old {
        println!(
            "  {} nothing to change — wado's hooks are {} {}",
            good("✓"),
            if install { "already in" } else { "not in" },
            path.display()
        );
        return 0;
    }
    let new_text = new.pretty();
    let old_shown = if std::fs::metadata(&path).is_ok() {
        old.pretty()
    } else {
        String::new()
    };
    println!(
        "\n  {} {}\n",
        bold(if install { "Adds to" } else { "Takes out of" }),
        path.display()
    );
    for l in diff::render(&old_shown, &new_text, 2) {
        let painted = match l.chars().next() {
            Some('+') => good(&l),
            Some('-') => bad(&l),
            _ => dim(&l),
        };
        println!("    {painted}");
    }
    println!();
    if install {
        println!(
            "  {}",
            dim(
                "Each hook runs `wado claude-event`, which tells the daemons on this computer what Claude is doing. A permission request waits up to 5 minutes for your phone, then Claude asks here as usual."
            )
        );
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "  not changed — run again with {} to write it",
                accent("--yes")
            );
            return 1;
        }
        print!("  Write this? [y/N] ");
        let _ = std::io::stdout().flush();
        let mut a = String::new();
        let _ = std::io::stdin().lock().read_line(&mut a);
        if !matches!(a.trim(), "y" | "Y" | "yes") {
            println!("  not changed");
            return 1;
        }
    }
    let backup = path.with_extension("json.wado-backup");
    if std::fs::metadata(&path).is_ok() && std::fs::metadata(&backup).is_err() {
        let _ = std::fs::copy(&path, &backup);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.wado-tmp");
    match std::fs::write(&tmp, new_text).and_then(|_| std::fs::rename(&tmp, &path)) {
        Ok(()) => {
            println!(
                "  {} {} — {}",
                good("✓"),
                if install { "installed" } else { "uninstalled" },
                if install {
                    "Claude sessions started from now on show on your phone"
                } else {
                    "your phone no longer hears from Claude"
                }
            );
            0
        }
        Err(e) => {
            eprintln!("  {} could not write {}: {e}", bad("✗"), path.display());
            1
        }
    }
}
