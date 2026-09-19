//! Reading one `.desktop` file.
//!
//! A `.desktop` file is an INI file, and everything needed from it is three keys in one group.
//! Reading them directly is about thirty lines; a spec-complete desktop-entry crate would be a
//! dependency for MIME associations and localisation that nothing here uses.
//!
//! Nothing in here touches the filesystem — that is [`super`]'s job, and the `Icon=` name is
//! handed on verbatim for [`super::icons`] to resolve. Keeping it pure is what lets every rule
//! below be tested against a string.
//!
//! Consciously *not* implemented, because none of it changes which entries a person sees in a
//! launcher list: `OnlyShowIn`/`NotShowIn` (there is no desktop environment name to match
//! against here), `DBusActivatable`, desktop actions, and localised `Name[xx]` variants.

/// One desktop entry, exactly as its file spells it.
///
/// Not [`wado_protocol::AppEntry`]: the wire type carries the icon as a `data:` URI, and
/// turning a name into one means reading the disk. Keeping the parsed shape separate is what
/// keeps this module pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    /// The `Icon=` value verbatim — a theme icon name, or an absolute path.
    pub icon: Option<String>,
    /// `NoDisplay=true` or `Hidden=true`: installed and launchable, but not for a menu.
    ///
    /// Reported rather than used to reject the entry. It used to reject it, and that was
    /// wrong in one specific way: half the desktop files on a normal machine carry it, and
    /// among them are the ones someone reaches a launcher *for* when the pretty list does
    /// not have what they want.
    pub no_display: bool,
}

/// Strip the `Exec=` field codes a launcher is expected to substitute.
///
/// `%f %F %u %U` are file/URL arguments, `%i %c %k` are icon/name/path expansions, and the
/// deprecated `%d %D %n %N %v %m` must be ignored outright. Nothing is being opened here, so
/// every one of them is dropped — passing `%U` through verbatim would hand the app a literal
/// "%U" as its first argument, which some apps treat as a filename and fail on.
///
/// `%%` is an escaped literal percent and survives as `%`.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(_) => {}          // a field code: drop the pair
            None => out.push('%'), // trailing '%' — leave it rather than invent a meaning
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse one desktop file. `None` when it is not something a person can launch.
///
/// "Can launch" is the only bar: a `Type=Application` with a name and a command. Whether it
/// *should be listed* is a separate question, answered by [`DesktopEntry::no_display`] and
/// decided by the client.
pub fn parse_entry(text: &str) -> Option<DesktopEntry> {
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut is_application = false;
    let mut no_display = false;

    // Only the `[Desktop Entry]` group counts; later groups are per-action overrides that
    // would otherwise overwrite the real Exec with an action's variant of it.
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("Type", v) => is_application = v == "Application",
            ("Name", v) => name = Some(v.to_string()),
            ("Exec", v) => exec = Some(strip_field_codes(v)),
            ("Icon", v) => icon = Some(v.to_string()),
            // NoDisplay means "installed, but not for humans to pick" — mime handlers,
            // helper stubs. Hidden means "deleted" per the spec. Both are carried, not
            // obeyed: see the field's docs.
            ("NoDisplay" | "Hidden", "true") => no_display = true,
            _ => {}
        }
    }

    let (name, exec) = (name?, exec?);
    (is_application && !name.is_empty() && !exec.is_empty()).then_some(DesktopEntry {
        name,
        exec,
        icon,
        no_display,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_field_codes_but_keeps_escaped_percent() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_field_codes("gimp-2.10 %U"), "gimp-2.10");
        assert_eq!(strip_field_codes("app -i %i -c %c %f"), "app -i -c");
        assert_eq!(strip_field_codes("printf 100%%"), "printf 100%");
        assert_eq!(strip_field_codes("weston-terminal"), "weston-terminal");
    }

    #[test]
    fn reads_name_and_exec_from_the_desktop_entry_group() {
        let app = parse_entry(
            "[Desktop Entry]\nType=Application\nName=Files\nExec=nautilus %U\nIcon=folder\n",
        )
        .expect("a normal entry should parse");
        assert_eq!(
            app,
            DesktopEntry {
                name: "Files".into(),
                exec: "nautilus".into(),
                // The name, not a resolved file: resolution happens a layer up.
                icon: Some("folder".into()),
                no_display: false,
            }
        );
    }

    #[test]
    fn later_groups_do_not_overwrite_exec() {
        // Desktop actions are a separate group with their own Exec. Reading the whole file
        // flat would launch "nautilus --new-window" under the name "Files".
        let app = parse_entry(
            "[Desktop Entry]\nType=Application\nName=Files\nExec=nautilus\n\
             [Desktop Action new-window]\nName=New Window\nExec=nautilus --new-window\n",
        )
        .unwrap();
        assert_eq!(app.exec, "nautilus");
        assert_eq!(app.name, "Files");
    }

    #[test]
    fn not_for_a_menu_is_reported_not_rejected() {
        // Both spellings, and both still parse: the client decides whether to list them.
        for key in ["NoDisplay", "Hidden"] {
            let app = parse_entry(&format!(
                "[Desktop Entry]\nType=Application\nName=X\nExec=x\n{key}=true\n"
            ))
            .expect("still launchable");
            assert!(app.no_display, "{key} must be carried");
            assert_eq!(app.exec, "x");
        }
        assert!(
            !parse_entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\n")
                .unwrap()
                .no_display
        );
    }

    #[test]
    fn skips_what_cannot_be_launched() {
        // Not an application.
        assert!(parse_entry("[Desktop Entry]\nType=Link\nName=Web\nExec=x\n").is_none());
        // Nothing to run, or nothing to show.
        assert!(parse_entry("[Desktop Entry]\nType=Application\nName=X\n").is_none());
        assert!(parse_entry("[Desktop Entry]\nType=Application\nExec=x\n").is_none());
    }
}
