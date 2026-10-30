//! Files the core keeps in its own directory, written atomically: `<name>.tmp`, fsync, rename.
//! A process killed mid-write leaves the old file whole, never half of the new one.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Replace `dir/name` with `bytes`, private to this user.
pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    {
        let mut o = fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dir.join(name))
}

/// `dir/name`, or `None` if it is not there.
pub fn read(dir: &Path, name: &str) -> Option<Vec<u8>> {
    fs::read(dir.join(name)).ok()
}

/// A fresh directory under the system's temp dir, for tests.
#[cfg(test)]
pub(crate) fn temp_dir(tag: &str) -> std::path::PathBuf {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 8];
    ring::rand::SystemRandom::new().fill(&mut b).unwrap();
    let d = std::env::temp_dir().join(format!("wado-core-{tag}-{}", u64::from_le_bytes(b)));
    fs::create_dir_all(&d).unwrap();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_replaces_whole_and_leaves_no_tmp() {
        let d = temp_dir("store");
        write(&d, "a.json", b"one").unwrap();
        write(&d, "a.json", b"two").unwrap();
        assert_eq!(read(&d, "a.json").unwrap(), b"two");
        assert!(!d.join("a.json.tmp").exists());
        assert!(read(&d, "missing").is_none());
        let _ = fs::remove_dir_all(d);
    }
}
