//! `wado claude hooks install|uninstall`: wado's entries in Claude Code's settings
//! (`~/.claude/settings.json`), added and taken out again without touching anything else.
//!
//! Each hook runs `wado claude-event`, which hands Claude's JSON to the daemons (see
//! [`super::hook_cli`]). Opt-in only, and always shown as a diff first (Decision Log
//! `2026-10-27`): it is the person's own config.

use super::ojson::J;

/// The events wado listens to, and each one's timeout in seconds. A permission request waits
/// for an answer from a phone, so it gets Claude's longest (600 s, its default); the rest only
/// report and return at once.
pub const EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 10),
    ("UserPromptSubmit", 10),
    ("PermissionRequest", 600),
    ("Notification", 10),
    ("Stop", 10),
    ("SessionEnd", 10),
];

/// Is this hook command one of wado's?
pub fn is_ours(command: &str) -> bool {
    let c = command.trim_end();
    c.ends_with("claude-event") && c.contains("wado")
}

/// The command a hook runs: this binary, quoted for the shell Claude runs hooks with.
pub fn command(exe: &str) -> String {
    let plain = exe
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c));
    if plain {
        format!("{exe} claude-event")
    } else {
        format!("'{}' claude-event", exe.replace('\'', "'\\''"))
    }
}

/// Has `wado claude hooks install` been run on these settings?
pub fn installed(settings: &J) -> bool {
    let Some(hooks) = settings.get("hooks") else {
        return false;
    };
    EVENTS.iter().all(|(event, _)| {
        matches!(hooks.get(event), Some(J::Arr(groups)) if groups.iter().any(group_is_ours))
    })
}

fn group_is_ours(g: &J) -> bool {
    matches!(g.get("hooks"), Some(J::Arr(hs)) if hs.iter().any(|h| h.get("command").and_then(J::as_str).is_some_and(is_ours)))
}

/// `settings` with wado's hooks taken out — and only them: someone else's hook in the same
/// group stays, and a list or object is removed only when wado's leaving emptied it.
pub fn uninstall(settings: &J) -> J {
    let mut s = settings.clone();
    let Some(hooks) = s.get_mut("hooks") else {
        return s;
    };
    let J::Obj(events) = hooks else {
        return s;
    };
    for (_, groups) in events.iter_mut() {
        let J::Arr(list) = groups else { continue };
        for g in list.iter_mut() {
            if let Some(J::Arr(hs)) = g.get_mut("hooks") {
                let before = hs.len();
                hs.retain(|h| !h.get("command").and_then(J::as_str).is_some_and(is_ours));
                if hs.is_empty() && before > 0 {
                    *g = J::Null; // emptied by us: dropped below
                }
            }
        }
        let before = list.len();
        list.retain(|g| *g != J::Null);
        if list.is_empty() && before > 0 {
            *groups = J::Null;
        }
    }
    events.retain(|(_, v)| *v != J::Null);
    if hooks.is_empty_container() {
        s.remove("hooks");
    }
    s
}

/// `settings` with wado's hooks in, once each — running it twice changes nothing.
pub fn install(settings: &J, command: &str) -> J {
    let mut s = uninstall(settings);
    if !matches!(s, J::Obj(_)) {
        s = J::obj();
    }
    if !matches!(s.get("hooks"), Some(J::Obj(_))) {
        s.set("hooks", J::obj());
    }
    let hooks = s.get_mut("hooks").expect("just set");
    for (event, timeout) in EVENTS {
        let mut entry = J::obj();
        entry.set("type", J::str("command"));
        entry.set("command", J::str(command));
        entry.set("timeout", J::num(*timeout));
        let mut group = J::obj();
        group.set("hooks", J::Arr(vec![entry]));
        match hooks.get_mut(event) {
            Some(J::Arr(list)) => list.push(group),
            _ => hooks.set(event, J::Arr(vec![group])),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEIRS: &str = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "their-thing"
          }
        ]
      }
    ]
  },
  "permissions": {
    "allow": [
      "Bash(ls)"
    ]
  }
}
"#;

    #[test]
    fn install_adds_ours_keeps_theirs_and_is_idempotent() {
        let old = J::parse(THEIRS).unwrap();
        let cmd = command("/usr/bin/wado");
        assert_eq!(cmd, "/usr/bin/wado claude-event");
        let new = install(&old, &cmd);
        assert!(installed(&new) && !installed(&old));
        assert_eq!(install(&new, &cmd), new, "a second install changes nothing");
        let text = new.pretty();
        assert!(text.contains("their-thing"), "someone else's hook stays");
        assert_eq!(
            text.matches("claude-event").count(),
            EVENTS.len(),
            "once per event"
        );
        // Their keys stay where they were.
        let order: Vec<_> = match &new {
            J::Obj(m) => m.iter().map(|(k, _)| k.as_str()).collect(),
            _ => vec![],
        };
        assert_eq!(order, ["model", "hooks", "permissions"]);
        // And the diff shown first is only additions.
        let shown = super::super::diff::render(THEIRS, &text, 1);
        assert!(!shown.is_empty());
        assert!(shown.iter().all(|l| !l.starts_with('-')), "{shown:#?}");
        assert!(shown.iter().any(|l| l.contains("PermissionRequest")));
    }

    #[test]
    fn uninstall_takes_out_only_ours() {
        let old = J::parse(THEIRS).unwrap();
        let new = install(&old, &command("/usr/bin/wado"));
        assert_eq!(uninstall(&new), old, "back to exactly what it was");
        assert_eq!(uninstall(&old), old, "nothing of ours: untouched");
        let bare = J::parse("{}").unwrap();
        let with = install(&bare, &command("/opt/my wado/wado"));
        assert!(with.pretty().contains(r"'/opt/my wado/wado' claude-event"));
        assert_eq!(uninstall(&with), bare, "the hooks object it made goes too");
    }
}
