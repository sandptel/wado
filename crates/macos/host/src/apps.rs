//! The Mac's own apps, with no session: what is installed, what runs (in front, hidden), what
//! the Mac keeps in its Dock and in which order — and opening, showing, hiding and quitting them
//! on the Mac's desktop.
//!
//! What runs is read from `lsappinfo` (Launch Services' own view: no permission needed, and live
//! — `NSWorkspace`'s list only updates on a main run loop this daemon does not spin). An app is
//! known by its bundle's file name (`Safari.app`), because one app has several paths: the Dock
//! names Safari inside its cryptex, `/Applications` holds a link to it.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use wado_platform::desktop::DeskApps;
use wado_platform::{Fut, Res};
use wado_protocol::{AppAction, AppEntry};

/// How long the installed list is trusted before the folders are read again.
const SCAN_EVERY: Duration = Duration::from_secs(60);
/// The icon's side, in pixels: a phone dock draws it at about 48 pt.
const ICON_PX: usize = 96;

/// One running app with a place in the Dock (`type="Foreground"`).
#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    pub asn: String,
    pub name: String,
    pub bundle_id: String,
    pub path: String,
    pub pid: i32,
    pub front: bool,
}

/// An app's key: its bundle's file name, lower case.
pub fn key(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

/// The foreground apps in `lsappinfo list`.
pub fn parse_running(out: &str) -> Vec<Running> {
    let mut all = Vec::new();
    let mut cur: Option<(Running, bool)> = None;
    let quoted = |l: &str, k: &str| -> Option<String> {
        let i = l.find(k)? + k.len();
        let rest = &l[i..];
        Some(rest[..rest.find('"')?].to_string())
    };
    for line in out.lines() {
        let l = line.trim_start();
        // ` 4) "Terminal" ASN:0x0-0x13013: (in front) `
        if let Some(rest) = l
            .split_once(") \"")
            .map(|(n, r)| (n, r))
            .filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
            .map(|(_, r)| r)
        {
            if let Some((r, fg)) = cur.take()
                && fg
            {
                all.push(r);
            }
            let name = rest[..rest.find('"').unwrap_or(0)].to_string();
            let asn = rest
                .split_whitespace()
                .find(|w| w.starts_with("ASN:"))
                .unwrap_or("")
                .to_string();
            cur = Some((
                Running {
                    asn,
                    name,
                    bundle_id: String::new(),
                    path: String::new(),
                    pid: 0,
                    front: l.contains("(in front)"),
                },
                false,
            ));
        } else if let Some((r, fg)) = cur.as_mut() {
            if let Some(v) = quoted(l, "bundleID=\"") {
                r.bundle_id = v;
            } else if let Some(v) = quoted(l, "bundle path=\"") {
                r.path = v;
            } else if l.starts_with("pid = ") {
                r.pid = l[6..]
                    .split_whitespace()
                    .next()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                *fg = l.contains("type=\"Foreground\"");
            }
        }
    }
    if let Some((r, true)) = cur {
        all.push(r);
    }
    all
}

/// The ASNs in `lsappinfo visibleProcessList`: the apps not hidden, front to back.
pub fn parse_visible(out: &str) -> Vec<String> {
    out.split("ASN:")
        .skip(1)
        .filter_map(|s| s.split('-').take(2).collect::<Vec<_>>().join("-").into())
        .map(|s: String| format!("ASN:{s}:"))
        .collect()
}

/// The apps kept in the Dock, in order, from `defaults read com.apple.dock persistent-apps`.
pub fn parse_dock(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let u = l.split("\"_CFURLString\" = \"").nth(1)?;
            let u = u.split('"').next()?;
            Some(percent_decode(u.strip_prefix("file://").unwrap_or(u)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The installed and the running, joined by key: every app once, sorted by name.
pub fn merge(
    installed: &[String],
    running: &[Running],
    visible: &[String],
    dock: &[String],
) -> Vec<AppEntry> {
    let mut by: BTreeMap<String, AppEntry> = BTreeMap::new();
    let entry = |path: &str| AppEntry {
        name: Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        exec: path.to_string(),
        ..Default::default()
    };
    for p in installed {
        by.entry(key(p)).or_insert_with(|| entry(p));
    }
    for r in running {
        let e = by.entry(key(&r.path)).or_insert_with(|| entry(&r.path));
        e.running = true;
        e.front = r.front;
        e.away = !visible.contains(&r.asn);
        if !r.name.is_empty() {
            e.name = r.name.clone();
        }
        if !r.bundle_id.is_empty() {
            e.app_ids = vec![r.bundle_id.clone()];
        }
    }
    for (i, p) in dock.iter().enumerate() {
        let e = by.entry(key(p)).or_insert_with(|| entry(p));
        e.dock = Some(i as u16);
    }
    let mut out: Vec<AppEntry> = by.into_values().collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn cmd(bin: &str, args: &[&str]) -> String {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Every `.app` in the usual folders, and one folder down (`/Applications/Sophos/…`).
fn scan() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = vec!["/System/Library/CoreServices/Finder.app".to_string()];
    for dir in [
        "/Applications".to_string(),
        "/Applications/Utilities".to_string(),
        "/System/Applications".to_string(),
        "/System/Applications/Utilities".to_string(),
        format!("{home}/Applications"),
    ] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".app") {
                out.push(p.to_string_lossy().into_owned());
            } else if p.is_dir() && !name.starts_with('.') && name != "Utilities" {
                for inner in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                    if inner.file_name().to_string_lossy().ends_with(".app") {
                        out.push(inner.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    out
}

#[derive(Default)]
pub struct MacApps {
    installed: Mutex<Option<(Instant, Vec<String>)>>,
}

impl MacApps {
    fn installed(&self) -> Vec<String> {
        let mut g = self.installed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, list)) = g.as_ref()
            && at.elapsed() < SCAN_EVERY
        {
            return list.clone();
        }
        let list = scan();
        *g = Some((Instant::now(), list.clone()));
        list
    }

    fn now(&self) -> Vec<AppEntry> {
        let running = parse_running(&cmd("/usr/bin/lsappinfo", &["list"]));
        let visible = parse_visible(&cmd("/usr/bin/lsappinfo", &["visibleProcessList"]));
        let dock = parse_dock(&cmd(
            "/usr/bin/defaults",
            &["read", "com.apple.dock", "persistent-apps"],
        ));
        merge(&self.installed(), &running, &visible, &dock)
    }
}

fn icon(path: &str) -> Option<String> {
    let img = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    let tiff = img.TIFFRepresentation()?;
    crate::art::png_uri(&tiff.to_vec(), ICON_PX)
}

/// The running app at `exec`, by its bundle's key.
fn find(exec: &str) -> Option<Running> {
    let k = key(exec);
    parse_running(&cmd("/usr/bin/lsappinfo", &["list"]))
        .into_iter()
        .find(|r| key(&r.path) == k)
}

fn act(exec: &str, action: AppAction) -> Res<()> {
    if !exec.ends_with(".app") || !Path::new(exec).exists() {
        return Err("That app is not on this Mac any more.".into());
    }
    match action {
        // `open -a` starts it, or brings it (and shows it, if hidden) to the front.
        AppAction::Open | AppAction::Unhide => {
            let ok = std::process::Command::new("/usr/bin/open")
                .args(["-a", exec])
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                Ok(())
            } else {
                Err("macOS would not open it.".into())
            }
        }
        AppAction::Hide | AppAction::Quit | AppAction::ForceQuit => {
            let r = find(exec).ok_or("It is not running.")?;
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(r.pid)
                .ok_or("It is not running.")?;
            let done = match action {
                AppAction::Hide => app.hide(),
                AppAction::Quit => app.terminate(),
                _ => app.forceTerminate(),
            };
            if done {
                Ok(())
            } else {
                Err(format!("{} did not take that.", r.name))
            }
        }
    }
}

impl DeskApps for MacApps {
    fn list(&self) -> Fut<'_, Res<Vec<AppEntry>>> {
        Box::pin(async move { Ok(self.now()) })
    }

    fn icons(&self, execs: Vec<String>) -> Fut<'_, BTreeMap<String, String>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut seen = HashMap::new();
                execs
                    .into_iter()
                    .filter_map(|e| {
                        let uri = seen.entry(key(&e)).or_insert_with(|| icon(&e)).clone()?;
                        Some((e, uri))
                    })
                    .collect()
            })
            .await
            .unwrap_or_default()
        })
    }

    fn act(&self, exec: String, action: AppAction) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || act(&exec, action))
                .await
                .map_err(|e| e.to_string())?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#" 3) "WindowManager" ASN:0x0-0xd00d:
    bundleID="com.apple.WindowManager"
    bundle path="/System/Library/CoreServices/WindowManager.app"
    pid = 824 type="UIElement" flavor=3 Version="341.6.1"
 4) "Terminal" ASN:0x0-0x13013: (in front)
    bundleID="com.apple.Terminal"
    bundle path="/System/Applications/Utilities/Terminal.app"
    pid = 867 type="Foreground" flavor=3 Version="470.2"
89) "Safari" ASN:0x0-0x1623622:
    bundleID="com.apple.Safari"
    bundle path="/System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app"
    pid = 13935 type="Foreground" flavor=3
"#;

    #[test]
    fn reads_lsappinfo() {
        let r = parse_running(LIST);
        assert_eq!(r.len(), 2, "UI elements are not dock apps: {r:?}");
        assert_eq!(
            (r[0].name.as_str(), r[0].pid, r[0].front),
            ("Terminal", 867, true)
        );
        assert_eq!(r[1].bundle_id, "com.apple.Safari");
        assert_eq!(
            parse_visible(r#"ASN:0x0-0x13013-"Terminal": ASN:0x0-0x88d88d-"Slack":"#),
            ["ASN:0x0-0x13013:", "ASN:0x0-0x88d88d:"]
        );
    }

    #[test]
    fn reads_the_dock() {
        let d = r#"            "file-label" = Safari;
                "_CFURLString" = "file:///System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app/";
                "_CFURLString" = "file:///Applications/Visual%20Studio%20Code.app/";"#;
        assert_eq!(
            parse_dock(d),
            [
                "/System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app/",
                "/Applications/Visual Studio Code.app/"
            ]
        );
    }

    #[test]
    fn merges_by_bundle_name() {
        let installed = vec![
            "/Applications/Safari.app".to_string(),
            "/Applications/Notes.app".to_string(),
        ];
        let running = parse_running(LIST);
        let dock = vec![
            "/System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app/".to_string(),
        ];
        let all = merge(
            &installed,
            &running,
            &["ASN:0x0-0x13013:".to_string()],
            &dock,
        );
        let by = |n: &str| all.iter().find(|a| a.name == n).unwrap().clone();
        assert_eq!(all.len(), 3, "Safari once, from three paths: {all:?}");
        let s = by("Safari");
        assert!(s.running && s.away && !s.front && s.dock == Some(0));
        assert_eq!(
            s.exec, "/Applications/Safari.app",
            "the installed path names it"
        );
        let t = by("Terminal");
        assert!(t.running && t.front && !t.away && t.dock.is_none());
        assert!(!by("Notes").running);
    }
}
