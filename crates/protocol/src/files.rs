//! The file manager's wire — the `files` data channel of the second peer connection
//! (`WADO_PLAN.md`, Decision Log `2026-10-04`). Server half: `crates/server/src/files/`; client
//! half: `crates/client/src/js/files_*.js`, which builds these by hand.
//!
//! Text frames are JSON. A request is `{"id": n, "op": …}`; every answer carries the same `id`
//! and either `"ok": true` with its fields or `"err": "why"`. Bytes travel as binary frames:
//! a 4-byte big-endian transfer id, then the data. The channel is ordered and reliable, and it
//! is DTLS whose fingerprint came through the sealed envelope, so it needs no crypto of its own.
//!
//! Paths are the host's absolute paths, always inside one of the roots `hello` lists.

use serde::{Deserialize, Serialize};

/// The name of the one data channel on the files peer connection.
pub const FILES_CHANNEL: &str = "files";

/// Bytes per binary frame, header excluded. Under webrtc-rs's 64 KiB SCTP message cap.
pub const CHUNK: usize = 60 * 1024;

/// One request, as the client sends it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileFrame {
    pub id: u32,
    #[serde(flatten)]
    pub req: FileReq,
}

/// What to do when an upload's or a copy's name is already taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clash {
    /// Refuse with `err: "exists"` — the client then asks the user.
    #[default]
    Fail,
    Replace,
    /// Keep both: the new one is renamed `name (1).ext`.
    Rename,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FileReq {
    /// Roots, home, this device's access. Always the first request.
    Hello,
    List {
        path: String,
    },
    /// Send a file from `offset` (a resume). Answered `{size, mtime}`, then binary frames,
    /// then `{done: true, sha256}` — the hash of the **whole** file, prefix included.
    Get {
        path: String,
        #[serde(default)]
        offset: u64,
    },
    /// A folder as an uncompressed zip stream. Answered `{size}` (exact), frames, `{done}`.
    /// Not resumable: a drop restarts it.
    Zip {
        path: String,
    },
    /// Receive a file into `dir`. Answered `{name, offset}`: the name it lands under and how
    /// much of it a previous attempt already wrote. Then binary frames, then [`FileReq::PutEnd`].
    Put {
        dir: String,
        /// May hold `/` for a file inside an uploaded folder; each part is checked.
        name: String,
        size: u64,
        #[serde(default)]
        clash: Clash,
    },
    /// The client's hash of the whole file. Answered `{ok}` once the bytes match and the file is
    /// in place, `{err}` otherwise.
    PutEnd {
        xfer: u32,
        sha256: String,
    },
    /// Stop a transfer (pause is a cancel plus a later resume).
    Cancel {
        xfer: u32,
    },
    Mkdir {
        path: String,
    },
    Rename {
        path: String,
        to: String,
    },
    /// To the freedesktop Trash — never a hard delete.
    Trash {
        paths: Vec<String>,
    },
    Move {
        paths: Vec<String>,
        dest: String,
        #[serde(default)]
        clash: Clash,
    },
    Copy {
        paths: Vec<String>,
        dest: String,
        #[serde(default)]
        clash: Clash,
    },
    /// Pinned folders and recent files.
    Quick,
    /// A small PNG of an image, base64, or `err` when there is none.
    Thumb {
        path: String,
    },
    /// Add a folder to the pinned ones (the GTK bookmarks file, shared with Nautilus).
    Pin {
        path: String,
    },
    Unpin {
        path: String,
    },
    /// The trusted devices and their file access — for an `rw`, QR-pinned device.
    Devices,
    /// Set a device's access: `none`, `ro` or `rw`. Only from an `rw`, QR-pinned device.
    Grant {
        key: String,
        level: String,
    },
    /// Search under `path`: names containing `query` (any case), and/or files of a `kind`
    /// (`image`, `video`, `audio`, `doc`, `archive`). Bounded — a few thousand folders at most —
    /// and newest first. `path` empty searches every root.
    Find {
        #[serde(default)]
        path: String,
        #[serde(default)]
        query: String,
        #[serde(default)]
        kind: String,
    },
    /// Put a trashed item (a path inside the Trash's `files/`) back where it came from.
    Restore {
        path: String,
    },
    /// What a video or audio file holds: duration, tracks, subtitles (embedded and beside it).
    Probe {
        path: String,
    },
    /// Play a video or audio file from `start` seconds as fragmented MP4 for Media Source
    /// Extensions: remuxed when the browser can play its codecs, transcoded otherwise. Answered
    /// `{mime}`, then binary frames, then `{done}`. It sends only as far as the device's credit —
    /// [`FileReq::Credit`] — so a film is not transcoded faster than it is watched.
    Stream {
        path: String,
        #[serde(default)]
        start: f64,
        /// Which audio track (0-based among audio tracks).
        #[serde(default)]
        audio: u32,
        /// The browser can decode HEVC, so it is copied rather than transcoded.
        #[serde(default)]
        hevc: bool,
        /// Bytes the device will take before it asks for more.
        #[serde(default)]
        credit: u64,
    },
    /// More room for a stream: `bytes` more may be sent.
    Credit {
        xfer: u32,
        bytes: u64,
    },
    /// A subtitle track as WebVTT: embedded track `track`, or the subtitle file `sidecar`.
    Subs {
        path: String,
        #[serde(default)]
        track: Option<u32>,
        #[serde(default)]
        sidecar: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_as_the_client_builds_them() {
        let f: FileFrame =
            serde_json::from_str(r#"{"id":3,"op":"get","path":"/home/a/x.txt","offset":10}"#)
                .unwrap();
        assert_eq!(f.id, 3);
        assert_eq!(
            f.req,
            FileReq::Get {
                path: "/home/a/x.txt".into(),
                offset: 10
            }
        );
        let f: FileFrame = serde_json::from_str(
            r#"{"id":4,"op":"put","dir":"/home/a","name":"d/y","size":5,"clash":"rename"}"#,
        )
        .unwrap();
        assert!(matches!(
            f.req,
            FileReq::Put {
                clash: Clash::Rename,
                ..
            }
        ));
        let f: FileFrame = serde_json::from_str(r#"{"id":1,"op":"hello"}"#).unwrap();
        assert_eq!(f.req, FileReq::Hello);
        assert!(serde_json::from_str::<FileFrame>(r#"{"id":1,"op":"rm_rf"}"#).is_err());
    }
}
