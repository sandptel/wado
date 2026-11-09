//! Opening a path under a shared folder without leaving it, on macOS.
//!
//! Linux does this in one call (`openat2` with `RESOLVE_BENEATH`); macOS has no such flag, so
//! the path is walked one component at a time from the folder's own descriptor, never by
//! name from `/`. Every component is opened with `O_NOFOLLOW`, so a symlink is seen rather than
//! followed. When links are allowed, its target is spliced into the walk: an absolute target, or
//! a `..` that would climb above the folder, is refused with `EXDEV` — the error Linux gives —
//! and a renamed parent cannot redirect a walk that only ever holds descriptors.
#![cfg(target_os = "macos")]

use std::collections::VecDeque;
use std::ffi::{CString, OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

/// More links than this in one walk is a loop.
const MAX_LINKS: u32 = 40;

/// Open `rel` under `root` with `flags`. `follow`: symlinks that stay inside are followed;
/// otherwise any symlink is refused (`ELOOP`).
pub fn open(root: &File, rel: &Path, flags: i32, follow: bool) -> io::Result<File> {
    let mut stack = vec![root.try_clone()?];
    let mut todo = parts(rel)?;
    let mut links = 0;
    while let Some(name) = todo.pop_front() {
        if name == ".." {
            if stack.len() == 1 {
                return Err(io::Error::from_raw_os_error(libc::EXDEV));
            }
            stack.pop();
            continue;
        }
        let last = todo.is_empty();
        let dir = stack.last().expect("the root is never popped");
        let how = if last {
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC
        } else {
            libc::O_EVTONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        };
        match openat(dir, &name, how) {
            Ok(f) if last => return Ok(f),
            Ok(f) => stack.push(f),
            Err(e) if is_link(dir, &name) => {
                if !follow {
                    return Err(io::Error::from_raw_os_error(libc::ELOOP));
                }
                links += 1;
                if links > MAX_LINKS {
                    return Err(io::Error::from_raw_os_error(libc::ELOOP));
                }
                let target = readlinkat(dir, &name).map_err(|_| e)?;
                if Path::new(&target).is_absolute() {
                    return Err(io::Error::from_raw_os_error(libc::EXDEV));
                }
                let mut spliced = parts(Path::new(&target))?;
                spliced.extend(todo.drain(..));
                todo = spliced;
            }
            Err(e) => return Err(e),
        }
    }
    // The path named the folder itself (empty, or `x/..`).
    openat(
        stack.last().expect("the root is never popped"),
        OsStr::new("."),
        flags | libc::O_CLOEXEC,
    )
}

/// The walkable parts of `p`: names and `..`; `.` and the root are dropped, so only a
/// relative walk is ever possible.
fn parts(p: &Path) -> io::Result<VecDeque<OsString>> {
    let mut out = VecDeque::new();
    for c in p.components() {
        match c {
            Component::Normal(n) => out.push_back(n.to_owned()),
            Component::ParentDir => out.push_back("..".into()),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {
                return Err(io::Error::from_raw_os_error(libc::EXDEV));
            }
        }
    }
    Ok(out)
}

fn cname(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

fn openat(dir: &File, name: &OsStr, flags: i32) -> io::Result<File> {
    let c = cname(name)?;
    // SAFETY: a valid directory fd and a NUL-terminated name; mode is read only with O_CREAT.
    let fd = unsafe { libc::openat(dir.as_raw_fd(), c.as_ptr(), flags, 0o666 as libc::c_uint) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel just handed us this descriptor and nothing else owns it.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn is_link(dir: &File, name: &OsStr) -> bool {
    let Ok(c) = cname(name) else { return false };
    // SAFETY: `stat` is plain data the kernel fills; valid fd and NUL-terminated name.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        libc::fstatat(
            dir.as_raw_fd(),
            c.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        ) == 0
            && st.st_mode & libc::S_IFMT == libc::S_IFLNK
    }
}

fn readlinkat(dir: &File, name: &OsStr) -> io::Result<OsString> {
    let c = cname(name)?;
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: valid fd, NUL-terminated name, and the buffer's real length.
    let n = unsafe {
        libc::readlinkat(
            dir.as_raw_fd(),
            c.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n as usize);
    Ok(OsString::from(OsStr::from_bytes(&buf)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tree() -> (std::path::PathBuf, File) {
        let d = std::env::temp_dir().join(format!(
            "wado-beneath-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        std::fs::create_dir_all(d.join("in/deep")).unwrap();
        std::fs::write(d.join("in/a.txt"), b"a").unwrap();
        std::fs::write(d.join("secret.txt"), b"s").unwrap();
        symlink("deep", d.join("in/ok")).unwrap();
        symlink("../secret.txt", d.join("in/out")).unwrap();
        symlink("/etc/hosts", d.join("in/abs")).unwrap();
        let root = File::open(d.join("in")).unwrap();
        (d, root)
    }

    fn err(r: io::Result<File>) -> i32 {
        r.err().and_then(|e| e.raw_os_error()).unwrap_or(0)
    }

    #[test]
    fn plain_paths_open() {
        let (_d, root) = tree();
        assert!(open(&root, Path::new("a.txt"), libc::O_RDONLY, false).is_ok());
        assert!(
            open(
                &root,
                Path::new("deep"),
                libc::O_RDONLY | libc::O_DIRECTORY,
                false
            )
            .is_ok()
        );
        assert!(open(&root, Path::new(""), libc::O_RDONLY, false).is_ok());
    }

    #[test]
    fn symlink_component_refused_without_follow() {
        let (_d, root) = tree();
        assert_eq!(
            err(open(&root, Path::new("ok"), libc::O_RDONLY, false)),
            libc::ELOOP
        );
    }

    #[test]
    fn inside_link_followed_when_allowed() {
        let (_d, root) = tree();
        assert!(
            open(
                &root,
                Path::new("ok"),
                libc::O_RDONLY | libc::O_DIRECTORY,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn escaping_links_refused_even_when_following() {
        let (_d, root) = tree();
        assert_eq!(
            err(open(&root, Path::new("out"), libc::O_RDONLY, true)),
            libc::EXDEV
        );
        assert_eq!(
            err(open(&root, Path::new("abs"), libc::O_RDONLY, true)),
            libc::EXDEV
        );
    }

    #[test]
    fn dotdot_and_absolute_refused() {
        let (_d, root) = tree();
        assert_eq!(
            err(open(
                &root,
                Path::new("../secret.txt"),
                libc::O_RDONLY,
                true
            )),
            libc::EXDEV
        );
        assert_eq!(
            err(open(&root, Path::new("/etc/hosts"), libc::O_RDONLY, true)),
            libc::EXDEV
        );
    }

    #[test]
    fn dotdot_inside_stays_inside() {
        let (_d, root) = tree();
        assert!(open(&root, Path::new("deep/../a.txt"), libc::O_RDONLY, false).is_ok());
    }
}
