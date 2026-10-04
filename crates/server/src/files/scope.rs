//! What a device may reach: every path is resolved **by the kernel** under its root, with
//! `openat2(RESOLVE_BENEATH)`, and the denylist is checked on the path that resolution actually
//! reached — never on the string the client sent.
//!
//! Why the kernel: checking a path string and then opening it is a race (a symlink swapped in
//! between) and a parser bug waiting to happen (`..`, `//`, symlinks to `/`). `RESOLVE_BENEATH`
//! fails any resolution that would leave the directory it starts from, symlinks included, so
//! there is nothing to get wrong here. Everything after it works on the descriptor it returned.
//!
//! Operations that create or move a name open the **parent** this way and then act on one
//! checked name inside it with the `*at` calls (`openat` with `O_NOFOLLOW`, `mkdirat`,
//! `renameat2`): the parent is pinned by its descriptor, and the name has no `/` to resolve
//! through.

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Component, Path, PathBuf};

use wado_config::schema::files::{ALWAYS_DENY, Files};

/// Why a request was refused, as the device is told it.
pub type Refusal = String;

#[derive(Debug, Clone)]
pub struct Scope {
    roots: Vec<PathBuf>,
    deny: Vec<PathBuf>,
    pub hidden: bool,
    follow: bool,
}

/// An opened file or directory, and where it really is.
pub struct Opened {
    pub file: File,
    pub real: PathBuf,
}

impl Opened {
    /// `name` inside this directory, as a path the kernel resolves through the descriptor.
    /// Valid only while `self` is alive.
    pub fn at(&self, name: &str) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}/{name}", self.file.as_raw_fd()))
    }
    /// This directory itself, the same way.
    pub fn here(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()))
    }

    /// Open one checked `name` inside this directory, never following a symlink at it.
    pub fn open_at(&self, name: &str, flags: i32, mode: u32) -> io::Result<Opened> {
        let c = cname(name)?;
        // SAFETY: valid dir fd and NUL-terminated name.
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                c.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NOCTTY | libc::O_NONBLOCK,
                mode,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Opened {
            // SAFETY: fresh descriptor, owned by nothing else.
            file: unsafe { File::from_raw_fd(fd) },
            real: self.real.join(name),
        })
    }

    pub fn mkdir_at(&self, name: &str) -> io::Result<()> {
        let c = cname(name)?;
        // SAFETY: valid dir fd and NUL-terminated name.
        check(unsafe { libc::mkdirat(self.file.as_raw_fd(), c.as_ptr(), 0o777) })
    }

    /// Move `name` here to `to_name` in `to`. `replace`: over an existing one; otherwise an
    /// existing name fails with `EEXIST` — checked by the kernel, not by a racy look first.
    pub fn rename_at(
        &self,
        name: &str,
        to: &Opened,
        to_name: &str,
        replace: bool,
    ) -> io::Result<()> {
        let (a, b) = (cname(name)?, cname(to_name)?);
        let flags = if replace { 0 } else { libc::RENAME_NOREPLACE };
        // SAFETY: valid dir fds and NUL-terminated names.
        check(unsafe {
            libc::renameat2(
                self.file.as_raw_fd(),
                a.as_ptr(),
                to.file.as_raw_fd(),
                b.as_ptr(),
                flags,
            )
        })
    }

    /// `name` here exists (a dangling symlink counts).
    pub fn has(&self, name: &str) -> bool {
        std::fs::symlink_metadata(self.at(name)).is_ok()
    }

    /// A free name like `name`: itself, else `name (1).ext`, `name (2).ext`, …
    pub fn free_name(&self, name: &str) -> String {
        if !self.has(name) {
            return name.into();
        }
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, ""),
        };
        (1..)
            .map(|n| format!("{stem} ({n}){ext}"))
            .find(|n| !self.has(n))
            .unwrap_or_default()
    }
}

fn cname(name: &str) -> io::Result<CString> {
    check_name(name).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    CString::new(name).map_err(|_| io::ErrorKind::InvalidInput.into())
}

fn check(r: i32) -> io::Result<()> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    }
}

/// One name inside a directory: no separators, not `.` or `..`, not empty, no NUL.
pub fn check_name(name: &str) -> Result<&str, Refusal> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(format!("`{name}` is not a valid name"));
    }
    if name.len() > 255 {
        return Err("that name is too long".into());
    }
    Ok(name)
}

fn openat2(dir: &File, rel: &Path, flags: i32, resolve: u64) -> io::Result<File> {
    use std::os::unix::ffi::OsStrExt;
    let rel = if rel.as_os_str().is_empty() {
        Path::new(".")
    } else {
        rel
    };
    let c = CString::new(rel.as_os_str().as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: `open_how` is plain data; zeroed is its documented "no options" state.
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = (flags | libc::O_CLOEXEC) as u64;
    how.resolve = resolve;
    // SAFETY: valid fd, NUL-terminated path, and a correctly sized `open_how`.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dir.as_raw_fd(),
            c.as_ptr(),
            &how as *const libc::open_how,
            std::mem::size_of::<libc::open_how>(),
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel just handed us this descriptor and nothing else owns it.
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}

fn real_of(f: &File) -> io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", f.as_raw_fd()))
}

pub fn say(e: io::Error) -> Refusal {
    match e.raw_os_error() {
        Some(libc::EXDEV) | Some(libc::ELOOP) => {
            "that path leads outside what this computer shares".into()
        }
        Some(libc::ENOENT) => "it is not there any more".into(),
        Some(libc::EACCES) | Some(libc::EPERM) => "this computer's permissions refuse it".into(),
        Some(libc::ENOTDIR) => "that is not a folder".into(),
        Some(libc::EEXIST) | Some(libc::ENOTEMPTY) => "exists".into(),
        Some(libc::ENOSPC) => "the computer's disk is full".into(),
        _ => e.to_string(),
    }
}

impl Scope {
    pub fn new(cfg: &Files) -> Self {
        let roots = cfg
            .roots()
            .iter()
            .filter_map(|r| std::fs::canonicalize(expand(r)).ok())
            .collect();
        let deny = ALWAYS_DENY
            .iter()
            .copied()
            .chain(cfg.deny.iter().map(String::as_str))
            .map(|d| {
                let p = expand(d);
                std::fs::canonicalize(&p).unwrap_or(p)
            })
            .collect();
        Self {
            roots,
            deny,
            hidden: cfg.hidden,
            follow: cfg.follow_symlinks != "never",
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Inside a denied tree.
    pub fn denied(&self, real: &Path) -> bool {
        self.deny.iter().any(|d| real.starts_with(d))
    }

    /// Holds a denied tree: moving, trashing or renaming it would carry the denied part out.
    pub fn holds_denied(&self, real: &Path) -> bool {
        self.deny.iter().any(|d| d.starts_with(real))
    }

    /// Shown in a listing: not denied, and not a dotfile unless `hidden`.
    pub fn shows(&self, dir_real: &Path, name: &str) -> bool {
        (self.hidden || !name.starts_with('.')) && !self.denied(&dir_real.join(name))
    }

    /// The root `path` lies under (the deepest, when roots nest), and the rest of it.
    fn split(&self, path: &str) -> Result<(&PathBuf, PathBuf), Refusal> {
        let p = Path::new(path);
        if !p.is_absolute() || path.contains('\0') {
            return Err("not an absolute path".into());
        }
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("`..` is not allowed".into());
        }
        self.roots
            .iter()
            .filter_map(|r| p.strip_prefix(r).ok().map(|rest| (r, rest.to_path_buf())))
            .max_by_key(|(r, _)| r.as_os_str().len())
            .ok_or_else(|| "that path is outside what this computer shares".into())
    }

    /// Open `path` with `flags` (`O_RDONLY`, `O_DIRECTORY`, …), resolved under its root.
    pub fn open(&self, path: &str, flags: i32) -> Result<Opened, Refusal> {
        let (root, rel) = self.split(path)?;
        let root = File::open(root).map_err(say)?;
        let mut resolve = libc::RESOLVE_BENEATH | libc::RESOLVE_NO_MAGICLINKS;
        if !self.follow {
            resolve |= libc::RESOLVE_NO_SYMLINKS;
        }
        // O_NONBLOCK: opening a FIFO for reading would otherwise hang this task forever.
        // Not with O_PATH, which opens nothing: openat2 refuses it any other flag (EINVAL).
        let extra = if flags & libc::O_PATH != 0 {
            0
        } else {
            libc::O_NOCTTY | libc::O_NONBLOCK
        };
        let file = openat2(&root, &rel, flags | extra, resolve).map_err(say)?;
        let real = real_of(&file).map_err(say)?;
        if self.denied(&real) {
            return Err("that path is private on this computer".into());
        }
        Ok(Opened { file, real })
    }

    pub fn dir(&self, path: &str) -> Result<Opened, Refusal> {
        self.open(path, libc::O_RDONLY | libc::O_DIRECTORY)
    }

    /// The directory holding `path`, and its last name — for creating, renaming or moving it.
    /// Refuses a root itself, and anything denied or holding a denied tree.
    pub fn parent(&self, path: &str) -> Result<(Opened, String), Refusal> {
        let p = Path::new(path);
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("that path has no name")?;
        let name = check_name(name)?.to_string();
        if self.roots.iter().any(|r| r == p) {
            return Err("a shared root cannot be changed itself".into());
        }
        let dir = self.dir(p.parent().and_then(Path::to_str).unwrap_or("/"))?;
        let real = dir.real.join(&name);
        if self.denied(&real) || self.holds_denied(&real) {
            return Err("that path is private on this computer".into());
        }
        Ok((dir, name))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A scope rooted at a temp dir `<t>/root`, with `<t>/outside` next to it.
    pub fn sandbox(name: &str) -> (PathBuf, Scope) {
        let t = std::env::temp_dir().join(format!("wado-scope-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join("root/docs")).unwrap();
        std::fs::create_dir_all(t.join("root/secret")).unwrap();
        std::fs::create_dir_all(t.join("outside")).unwrap();
        std::fs::write(t.join("root/docs/a.txt"), "a").unwrap();
        std::fs::write(t.join("root/secret/key"), "k").unwrap();
        std::fs::write(t.join("outside/passwd"), "p").unwrap();
        let t = std::fs::canonicalize(&t).unwrap();
        let cfg = Files {
            root: vec![t.join("root").to_string_lossy().into()],
            deny: vec![t.join("root/secret").to_string_lossy().into()],
            ..Files::default()
        };
        (t, Scope::new(&cfg))
    }

    fn s(p: PathBuf) -> String {
        p.to_string_lossy().into()
    }

    #[test]
    fn nothing_escapes_its_root() {
        let (t, sc) = sandbox("escape");
        let r = t.join("root");
        assert!(sc.open(&s(r.join("docs/a.txt")), libc::O_RDONLY).is_ok());
        // Lexical tricks.
        assert!(
            sc.open(&s(r.join("docs/../../outside/passwd")), libc::O_RDONLY)
                .is_err()
        );
        assert!(
            sc.open(&s(t.join("outside/passwd")), libc::O_RDONLY)
                .is_err()
        );
        assert!(sc.open("docs/a.txt", libc::O_RDONLY).is_err());
        // A symlink that leaves the root: the kernel refuses it, absolute or relative.
        symlink(t.join("outside"), r.join("abs")).unwrap();
        symlink("../outside", r.join("rel")).unwrap();
        assert!(sc.open(&s(r.join("abs/passwd")), libc::O_RDONLY).is_err());
        assert!(sc.open(&s(r.join("rel/passwd")), libc::O_RDONLY).is_err());
        // One that stays inside is fine ("inside")…
        symlink("docs", r.join("in")).unwrap();
        assert!(sc.open(&s(r.join("in/a.txt")), libc::O_RDONLY).is_ok());
        assert!(
            sc.open(&s(r.join("in/a.txt")), libc::O_PATH).is_ok(),
            "O_PATH, as listings use"
        );
        // …but not into the denied tree, by any name.
        symlink("secret", r.join("sneaky")).unwrap();
        assert!(sc.open(&s(r.join("secret/key")), libc::O_RDONLY).is_err());
        assert!(sc.open(&s(r.join("sneaky/key")), libc::O_RDONLY).is_err());
        assert!(sc.parent(&s(r.join("secret"))).is_err());
        assert!(!sc.shows(&r, "secret"));
    }

    #[test]
    fn a_tree_holding_a_denied_one_cannot_be_moved() {
        let (t, sc) = sandbox("holds");
        let r = t.join("root");
        std::fs::create_dir_all(r.join("cfg/wado")).unwrap();
        let cfg = Files {
            root: vec![s(r.clone())],
            deny: vec![s(r.join("cfg/wado"))],
            ..Files::default()
        };
        let sc2 = Scope::new(&cfg);
        assert!(
            sc2.parent(&s(r.join("cfg"))).is_err(),
            "would carry cfg/wado out"
        );
        assert!(sc2.parent(&s(r.join("docs"))).is_ok());
        assert!(sc.parent(&s(r.clone())).is_err(), "a root itself");
        assert!(check_name("a/b").is_err() && check_name("..").is_err());
    }
}
