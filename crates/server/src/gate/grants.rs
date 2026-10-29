//! What a trusted device may do beyond seeing and driving the desktop.
//!
//! Written as tokens — `files-ro`, `files-rw`, `shells`, `settings`, `host`, `input`, `clipboard` —
//! in three places that must agree: `security { new-device }`, a pairing code's checklist, and
//! `wado allow`.
//! On a device's `trusted_clients` line, files keep their own column (5) and the rest are
//! column 6, comma-joined, `-` for none. A line without column 6 predates grants and keeps what
//! it had then: shells and host controls.
//!
//! The owner device may do everything, whatever its line says.

pub const TOKENS: &[&str] = &[
    "files-ro",
    "files-rw",
    "shells",
    "settings",
    "host",
    "input",
    "clipboard",
    "join",
    "takeover",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    /// `none`, `ro` or `rw`. Counts only on a QR-pinned line (see `Gate::files_access`).
    pub files: &'static str,
    /// The console's shells — daemon-owned, so they hold whatever the owner left open.
    pub shells: bool,
    /// Privileged settings and Wi-Fi: what can run code or cut the computer off.
    pub settings: bool,
    /// The computer's sound, Bluetooth and sleep.
    pub host: bool,
    /// Typing and touching into the host desktop — terminal-equivalent power. Off by default,
    /// including for every line from before this grant existed.
    pub input: bool,
    /// The computer's clipboard history — it holds whatever was copied there, passwords too.
    pub clipboard: bool,
    /// Join a session another device is in, and control it with them (Decision Log
    /// `2026-10-30`): it sees everything on that screen.
    pub join: bool,
    /// Take a session over: become its lead and disconnect everyone else in it.
    pub takeover: bool,
}

impl Grants {
    pub const NONE: Self = Self {
        files: "none",
        shells: false,
        settings: false,
        host: false,
        input: false,
        clipboard: false,
        join: false,
        takeover: false,
    };
    pub const ALL: Self = Self {
        files: "rw",
        shells: true,
        settings: true,
        host: true,
        input: true,
        clipboard: true,
        join: true,
        takeover: true,
    };

    /// What a line from before grants had.
    pub const LEGACY: Self = Self {
        files: "none",
        shells: true,
        settings: false,
        host: true,
        input: false,
        clipboard: false,
        join: false,
        takeover: false,
    };

    /// What pairing with a code adds, whatever the code's checklist said (Decision Log
    /// `2026-10-30`, S1): a device proven to have seen this computer's own screen is one of its
    /// owner's, so it may join and take over the owner's sessions. Any other line gets them only
    /// from `wado allow`.
    pub const CODE_PAIRED: Self = Self {
        join: true,
        takeover: true,
        ..Self::NONE
    };

    /// `security { new-device }`: what a newly trusted device starts with.
    pub fn new_device() -> Self {
        Self::parse(&wado_config::live::current().security.new_device).unwrap_or(Self::NONE)
    }

    /// Tokens separated by spaces or commas. `-` or nothing is no grants.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut g = Self::NONE;
        for t in s.split([' ', ',']).filter(|t| !t.is_empty() && *t != "-") {
            g.set(t, true)?;
        }
        Ok(g)
    }

    /// Turn one token on or off. Off for either files token is no file access.
    pub fn set(&mut self, token: &str, on: bool) -> Result<(), String> {
        match (token, on) {
            ("files-ro", true) => self.files = "ro",
            ("files-rw", true) => self.files = "rw",
            ("files" | "files-ro" | "files-rw", false) => self.files = "none",
            ("shells", _) => self.shells = on,
            ("settings", _) => self.settings = on,
            ("host", _) => self.host = on,
            ("input", _) => self.input = on,
            ("clipboard", _) => self.clipboard = on,
            ("join", _) => self.join = on,
            ("takeover", _) => self.takeover = on,
            _ => {
                return Err(format!(
                    "`{token}` is not a grant — use {}",
                    TOKENS.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Everything either side has — a re-pair never takes a grant away.
    pub fn union(&self, o: &Self) -> Self {
        let rank = |f: &str| {
            ["none", "ro", "rw"]
                .iter()
                .position(|l| *l == f)
                .unwrap_or(0)
        };
        Self {
            files: if rank(o.files) > rank(self.files) {
                o.files
            } else {
                self.files
            },
            shells: self.shells || o.shells,
            settings: self.settings || o.settings,
            host: self.host || o.host,
            input: self.input || o.input,
            clipboard: self.clipboard || o.clipboard,
            join: self.join || o.join,
            takeover: self.takeover || o.takeover,
        }
    }

    /// Column 6: the grants other than files.
    pub fn flags(&self) -> String {
        let f: Vec<&str> = [
            (self.shells, "shells"),
            (self.settings, "settings"),
            (self.host, "host"),
            (self.input, "input"),
            (self.clipboard, "clipboard"),
            (self.join, "join"),
            (self.takeover, "takeover"),
        ]
        .into_iter()
        .filter_map(|(on, t)| on.then_some(t))
        .collect();
        if f.is_empty() {
            "-".into()
        } else {
            f.join(",")
        }
    }

    /// Columns 5 and 6 read back. `flags` is `None` on a line from before grants.
    pub fn from_columns(files: &str, flags: Option<&str>) -> Self {
        let mut g = match flags {
            Some(f) => Self::parse(f).unwrap_or(Self::NONE),
            None => Self::LEGACY,
        };
        g.files = level(files);
        g
    }

    /// All of it as tokens, for a pairing code.
    pub fn tokens(&self) -> String {
        let files = match self.files {
            "none" => None,
            l => Some(format!("files-{l}")),
        };
        let flags = self.flags();
        files
            .into_iter()
            .chain((flags != "-").then_some(flags))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// A files level as written, `none` unless it is one.
pub fn level(s: &str) -> &'static str {
    match s {
        "ro" => "ro",
        "rw" => "rw",
        _ => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip() {
        let g = Grants::parse("files-rw, shells host clipboard").unwrap();
        assert_eq!(g.files, "rw");
        assert!(g.shells && g.host && g.clipboard && !g.settings);
        assert_eq!(Grants::parse(&g.tokens()).unwrap(), g);
        assert_eq!(Grants::from_columns("rw", Some(&g.flags())), g);
        assert_eq!(Grants::parse("-").unwrap(), Grants::NONE);
        assert!(Grants::parse("root").is_err());
    }

    #[test]
    fn an_old_line_keeps_what_it_had() {
        assert_eq!(Grants::from_columns("", None), Grants::LEGACY);
    }

    #[test]
    fn a_re_pair_only_adds() {
        let had = Grants::parse("files-rw shells").unwrap();
        let code = Grants::parse("files-ro host").unwrap();
        let u = had.union(&code);
        assert_eq!(u.files, "rw");
        assert!(u.shells && u.host);
    }

    #[test]
    fn input_is_its_own_token_and_off_by_default() {
        let g = Grants::parse("host input").unwrap();
        assert!(g.input && g.host && !g.shells);
        assert_eq!(Grants::parse(&g.tokens()).unwrap(), g);
        assert!(!Grants::LEGACY.input);
        assert!(!Grants::parse("host").unwrap().input);
        assert!(Grants::ALL.input);
    }

    #[test]
    fn join_and_takeover_are_tokens_and_off_for_old_lines() {
        let g = Grants::parse("host join takeover").unwrap();
        assert!(g.join && g.takeover);
        assert_eq!(Grants::parse(&g.tokens()).unwrap(), g);
        assert_eq!(Grants::from_columns("none", Some(&g.flags())), g);
        // A line from before grants, and one from before these two: neither may join.
        assert!(!Grants::LEGACY.join && !Grants::LEGACY.takeover);
        let before = Grants::from_columns("none", Some("shells,host"));
        assert!(!before.join && !before.takeover);
        // A code adds them, and only them.
        let paired = before.union(&Grants::CODE_PAIRED);
        assert!(paired.join && paired.takeover && paired.shells && !paired.input);
    }
}
