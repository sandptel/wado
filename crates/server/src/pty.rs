//! An interactive shell on a pseudo-terminal.
//!
//! [`crate::exec`] runs one command with pipes and no terminal, which is enough to see the
//! output of `ls` and not much else. Nearly everything that makes a shell feel like a shell
//! is conditional on being attached to a terminal: job control, line editing and history,
//! colour, the prompt itself, and every full-screen program. A pipe gets none of it, and
//! programs detect the difference with `isatty` and deliberately behave differently.
//!
//! So this opens a real PTY, puts a login shell on the slave side, and ships the master's
//! bytes to a terminal emulator in the browser. The compositor's `WAYLAND_DISPLAY` is
//! inherited, so a GUI application started from this shell appears on the stream.
//!
//! **The reads are blocking and that is not negotiable** — a PTY master has no async
//! interface here — so the reader lives on its own thread and hands text to tokio through a
//! channel. Writes go the other way and are not worth a thread: a keystroke is a handful of
//! bytes into a kernel buffer.

use std::io::{Read, Write};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// How much to read from the master at a time.
const READ_CHUNK: usize = 8192;

/// The most bytes to hold back waiting for a UTF-8 character to complete.
///
/// A UTF-8 character is at most four bytes, so a tail longer than three is not a truncated
/// character — it is something that will never become valid, and holding it forever would
/// silently stop the terminal.
const MAX_PARTIAL: usize = 3;

/// A running shell: the master side of its terminal, and a handle to kill it.
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl Pty {
    /// Open a PTY, start the user's shell on it, and stream its output into `out`.
    ///
    /// The channel closing is what stops the reader thread: it is the signal that the viewer
    /// has gone.
    pub fn open(cols: u16, rows: u16, out: mpsc::Sender<String>) -> std::io::Result<Self> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| std::io::Error::other(format!("openpty: {e}")))?;

        // The user's own shell, as a login shell, so their prompt and aliases are there.
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(home);
        }
        // Without a TERM the shell assumes a dumb terminal and turns off everything the PTY
        // was opened for. 256-colour because that is what the browser emulator implements.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| std::io::Error::other(format!("spawn shell: {e}")))?;
        // Dropped explicitly: while the slave fd is open in this process the master never
        // reports EOF, so the reader would block forever after the shell exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::other(format!("clone reader: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::other(format!("take writer: {e}")))?;

        std::thread::Builder::new()
            .name("wado-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; READ_CHUNK];
                let mut partial: Vec<u8> = Vec::new();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // shell exited and the master hit EOF
                        Ok(n) => {
                            partial.extend_from_slice(&buf[..n]);
                            let text = take_text(&mut partial);
                            if !text.is_empty() && out.blocking_send(text).is_err() {
                                break; // viewer gone
                            }
                        }
                        Err(e) => {
                            // EIO here is the normal way a PTY master reports that the last
                            // slave closed — an exit, not a fault.
                            debug!("pty reader finished: {e}");
                            break;
                        }
                    }
                }
                debug!("pty reader thread ended");
            })?;

        info!(shell, cols, rows, "pty opened");
        Ok(Self {
            master: pair.master,
            writer,
            child,
        })
    }

    /// Send keystrokes to the shell.
    pub fn write(&mut self, data: &str) -> std::io::Result<()> {
        self.writer.write_all(data.as_bytes())?;
        self.writer.flush()
    }

    /// Tell the shell the terminal changed size. Full-screen programs redraw on this.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Err(e) = self.master.resize(size) {
            warn!("pty resize failed: {e}");
        }
    }

    /// Has the shell exited? Returns its code once it has.
    pub fn exited(&mut self) -> Option<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(Some(status.exit_code() as i32)),
            Ok(None) => None,
            Err(_) => Some(None),
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Killing the shell closes the last slave handle, and the kernel sends SIGHUP to
        // everything still on this terminal — which is how a closed terminal is supposed to
        // take its jobs with it.
        let _ = self.child.kill();
        let _ = self.child.wait();
        debug!("pty closed");
    }
}

/// Take everything from `buf` that forms complete UTF-8, leaving a truncated tail behind.
///
/// A read can end in the middle of a multi-byte character. Decoding each read independently
/// would turn that character into replacement characters and corrupt the next one too, which
/// shows up as mojibake in box-drawing and any non-ASCII prompt. Anything that is invalid
/// rather than merely incomplete is consumed with a replacement character, because leaving
/// it would stall every byte behind it forever.
fn take_text(buf: &mut Vec<u8>) -> String {
    let mut out = String::new();
    loop {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                out.push_str(s);
                buf.clear();
                return out;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                // Safe by construction: `valid_up_to` is exactly the length of the valid prefix.
                out.push_str(std::str::from_utf8(&buf[..valid]).unwrap_or(""));
                match e.error_len() {
                    // Genuinely invalid — drop it and keep going.
                    Some(bad) => {
                        out.push('\u{FFFD}');
                        buf.drain(..valid + bad);
                    }
                    // Truncated: keep it for the next read, unless it is too long to ever
                    // become a character.
                    None => {
                        buf.drain(..valid);
                        if buf.len() > MAX_PARTIAL {
                            out.push('\u{FFFD}');
                            buf.clear();
                        }
                        return out;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_passes_straight_through() {
        let mut buf = b"hello".to_vec();
        assert_eq!(take_text(&mut buf), "hello");
        assert!(buf.is_empty());
    }

    #[test]
    fn a_character_split_across_reads_is_rejoined_not_corrupted() {
        // "é" is 0xC3 0xA9. A read ending between them must not emit a replacement char.
        let mut buf = vec![b'a', 0xC3];
        assert_eq!(
            take_text(&mut buf),
            "a",
            "emitted the truncated byte instead of holding it"
        );
        assert_eq!(buf, vec![0xC3]);

        buf.push(0xA9);
        assert_eq!(take_text(&mut buf), "é");
        assert!(buf.is_empty());
    }

    #[test]
    fn a_four_byte_character_split_anywhere_survives() {
        // U+1F600, 0xF0 0x9F 0x98 0x80 — split at each of the three interior points.
        let full = "😀".as_bytes();
        for split in 1..4 {
            let mut buf = full[..split].to_vec();
            assert_eq!(take_text(&mut buf), "");
            buf.extend_from_slice(&full[split..]);
            assert_eq!(take_text(&mut buf), "😀", "split at {split}");
        }
    }

    #[test]
    fn invalid_bytes_are_consumed_rather_than_blocking_everything_behind_them() {
        // A lone 0xFF can never start a character. If it were held back as "incomplete",
        // the terminal would stop dead at the first byte of binary output.
        let mut buf = vec![b'a', 0xFF, b'b'];
        assert_eq!(take_text(&mut buf), "a\u{FFFD}b");
        assert!(buf.is_empty());
    }

    #[test]
    fn a_tail_too_long_to_be_a_character_is_not_held_forever() {
        // 4 continuation bytes with no lead byte: never valid, never completable.
        let mut buf = vec![0x80, 0x80, 0x80, 0x80];
        let out = take_text(&mut buf);
        assert!(out.contains('\u{FFFD}'), "{out:?}");
        assert!(buf.len() <= MAX_PARTIAL, "left {} bytes stuck", buf.len());
    }

    #[test]
    fn control_characters_survive_intact() {
        // Escape sequences are the whole point — they must not be mangled or dropped.
        let mut buf = b"\x1b[31mred\x1b[0m\x07".to_vec();
        assert_eq!(take_text(&mut buf), "\x1b[31mred\x1b[0m\x07");
    }
}
