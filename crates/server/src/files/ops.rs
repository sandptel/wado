//! New folder, rename, move and copy — all inside the scope, all on descriptors (see
//! [`super::scope`]). Delete is not here: it is [`super::trash`], never a hard delete.

use std::io::{Read, Write};

use wado_protocol::files::{CHUNK, Clash};

use super::scope::{Opened, Refusal, Scope, check_name, say};

pub fn mkdir(sc: &Scope, path: &str) -> Result<(), Refusal> {
    let (dir, name) = sc.parent(path)?;
    dir.mkdir_at(&name).map_err(say)
}

pub fn rename(sc: &Scope, path: &str, to: &str) -> Result<(), Refusal> {
    let to = check_name(to)?;
    let (dir, name) = sc.parent(path)?;
    if sc.denied(&dir.real.join(to)) {
        return Err("that name is private on this computer".into());
    }
    dir.rename_at(&name, &dir, to, false).map_err(say)
}

/// Move or copy `path` into the folder `dest`.
pub fn transfer(
    sc: &Scope,
    path: &str,
    dest: &str,
    clash: Clash,
    copy: bool,
) -> Result<(), Refusal> {
    let (from, name) = sc.parent(path)?;
    let to = sc.dir(dest)?;
    if to.real.starts_with(from.real.join(&name)) {
        return Err("a folder cannot go inside itself".into());
    }
    let mut target = name.clone();
    if to.has(&target) {
        match clash {
            Clash::Fail => return Err("exists".into()),
            Clash::Rename => target = to.free_name(&target),
            // Whatever was there goes to the Trash, not into oblivion.
            Clash::Replace => {
                super::trash::trash(sc, &format!("{}/{target}", dest.trim_end_matches('/')))?
            }
        }
    }
    if !copy {
        match from.rename_at(&name, &to, &target, false) {
            Ok(()) => return Ok(()),
            // Another filesystem: copy, then remove the original.
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {}
            Err(e) => return Err(say(e)),
        }
    }
    copy_into(sc, &from, &name, &to, &target)?;
    if !copy {
        let gone = from.at(&name);
        let r = if std::fs::symlink_metadata(&gone).is_ok_and(|m| m.is_dir()) {
            std::fs::remove_dir_all(&gone)
        } else {
            std::fs::remove_file(&gone)
        };
        r.map_err(|e| format!("copied, but the original could not be removed: {}", say(e)))?;
    }
    Ok(())
}

/// Copy `name` in `from` to `to_name` in `to`, a folder recursively. Symlinks are recreated as
/// they are (a link pointing out of the scope is still refused when anything opens it); denied
/// trees inside are left out.
fn copy_into(
    sc: &Scope,
    from: &Opened,
    name: &str,
    to: &Opened,
    to_name: &str,
) -> Result<(), Refusal> {
    let md = std::fs::symlink_metadata(from.at(name)).map_err(say)?;
    if sc.denied(&from.real.join(name)) {
        return Ok(());
    }
    if md.file_type().is_symlink() {
        let target = std::fs::read_link(from.at(name)).map_err(say)?;
        return std::os::unix::fs::symlink(target, to.at(to_name)).map_err(say);
    }
    if md.is_dir() {
        to.mkdir_at(to_name).map_err(say)?;
        let src = from
            .open_at(name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .map_err(say)?;
        let dst = to
            .open_at(to_name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .map_err(say)?;
        for e in std::fs::read_dir(src.here()).map_err(say)?.flatten() {
            if let Ok(n) = e.file_name().into_string() {
                copy_into(sc, &src, &n, &dst, &n)?;
            }
        }
        return Ok(());
    }
    if !md.is_file() {
        return Ok(()); // sockets, fifos, devices: not copied
    }
    let mut src = from.open_at(name, libc::O_RDONLY, 0).map_err(say)?.file;
    let mut dst = to
        .open_at(
            to_name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o666,
        )
        .map_err(say)?
        .file;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = src.read(&mut buf).map_err(say)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).map_err(say)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_stay_in_scope_and_never_clobber() {
        let (t, sc) = crate::files::scope::tests::sandbox("ops");
        let r = t.join("root");
        let p = |x: &str| r.join(x).to_string_lossy().into_owned();
        mkdir(&sc, &p("new")).unwrap();
        assert!(mkdir(&sc, &p("new")).is_err(), "exists");
        rename(&sc, &p("new"), "renamed").unwrap();
        assert!(rename(&sc, &p("renamed"), "../escape").is_err());
        assert!(
            rename(&sc, &p("renamed"), "secret").is_err(),
            "onto a denied name"
        );
        transfer(&sc, &p("docs/a.txt"), &p("renamed"), Clash::Fail, true).unwrap();
        assert!(transfer(&sc, &p("docs/a.txt"), &p("renamed"), Clash::Fail, true).is_err());
        transfer(&sc, &p("docs/a.txt"), &p("renamed"), Clash::Rename, true).unwrap();
        assert!(r.join("renamed/a (1).txt").exists());
        transfer(&sc, &p("docs"), &p("renamed"), Clash::Fail, false).unwrap();
        assert!(r.join("renamed/docs/a.txt").exists() && !r.join("docs").exists());
        assert!(transfer(&sc, &p("renamed"), &p("renamed/docs"), Clash::Fail, false).is_err());
        assert!(transfer(&sc, &p("secret/key"), &p("renamed"), Clash::Fail, true).is_err());
        assert!(
            transfer(
                &sc,
                &p("renamed"),
                &t.join("outside").to_string_lossy(),
                Clash::Fail,
                false
            )
            .is_err()
        );
    }
}
