//! Several devices in one session (Decision Log `2026-10-30`; spec
//! `docs/superpowers/specs/2026-10-30-multi-client-design.md`). The [`Hub`] is this daemon's one
//! session as its participants see it:
//!
//! | file | job |
//! |---|---|
//! | `mod.rs` | the hub: participants' handles, and every rule applied in one place |
//! | [`pc`] | one participant's peer connection, bound to the shared tracks |
//! | [`aggregate`] | everyone's attached / visible / strain / bitrate folded into one |
//! | [`presence`] | ghost cursors |
//! | [`room`] | a joined device's own room (`control=1&strict=1`, `room_hello{view}`) |
//! | [`forward`] | the session's state, pushed to a joined room |
//!
//! The seat's viewer (`relay_client`) and every joined room are participants alike: each has an
//! [`Origin`] the server gave it, its own sealed link (`out`), its own grants, and its own peer
//! connection carrying the session's one encode.

pub mod aggregate;
pub mod forward;
pub mod pc;
pub mod presence;
pub mod room;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use webrtc::api::API;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use wado_compositor::{CommandSender, CompositorCommand, InputEvent, InputSender};
use wado_protocol::multi::{LinkVerdict, Participant};
use wado_protocol::relay::RelayMsg;
use wado_protocol::{Quality, SessionConfig, SessionInfo};

use crate::linktest::verdict::Intent;
use crate::session::membership::{Member, Membership, Origin, hue};

/// A joined participant whose peer connection stays down this long is dropped from the session
/// (the session itself is kept: that is `viewer_watchdog`'s decision).
const PARTICIPANT_GONE: Duration = Duration::from_secs(45);
/// A speed-test verdict counts for the join or start it was about for this long.
const VERDICT_FOR: Duration = Duration::from_secs(5 * 60);

/// The session's state feeds, for a joined room to be sent what the seat is.
pub struct Feeds {
    pub text_input: watch::Receiver<bool>,
    pub shedding: watch::Receiver<u32>,
    pub windows: watch::Receiver<wado_protocol::WindowList>,
    pub menu: watch::Receiver<Option<wado_compositor::hit::MenuSpot>>,
    pub clipboard: watch::Receiver<wado_compositor::clipboard::Clip>,
    pub sent_kbps: watch::Receiver<u32>,
    pub timings: watch::Receiver<wado_protocol::StageTimings>,
    pub queue_us: Arc<AtomicU64>,
    pub a11y: Arc<crate::a11y::A11y>,
}

struct Peer {
    key: String,
    name: String,
    /// Its sealed link: the seat's relay writer, or its room's.
    out: mpsc::Sender<String>,
    /// The relay room, when it is the seat's viewer — a displaced seat is freed, not held.
    seat_room: Option<String>,
    pc: Option<Arc<RTCPeerConnection>>,
    presence: Option<Arc<RTCDataChannel>>,
    connected: bool,
    audio: bool,
    video: bool,
    /// A member whose connection is not up, since.
    down_since: Option<Instant>,
}

#[derive(Default)]
struct State {
    members: Membership,
    agg: aggregate::Aggregate,
    board: presence::Board,
    peers: HashMap<Origin, Peer>,
    /// The lead's own session config: what the output is built to.
    lead_config: Option<SessionConfig>,
    /// The bitrate the lead asked for, kbps (0 = not known yet).
    lead_kbps: u32,
    /// The bitrate in force for everyone.
    applied_kbps: u32,
    /// Each device's last speed test: what for, the answer, when.
    verdicts: HashMap<String, (Intent, LinkVerdict, Instant)>,
    /// The cap a speed test lowered the configured one to, and when.
    fixed_max: Option<(u32, Instant)>,
    /// The running session's encoder: `hardware` or `software`.
    encoder: String,
}

pub struct Hub {
    pub api: Arc<API>,
    pub track: Arc<TrackLocalStaticSample>,
    pub audio_track: Arc<TrackLocalStaticSample>,
    pub cmd_tx: CommandSender,
    pub input_tx: InputSender,
    pub listening: watch::Sender<bool>,
    pub audio_low: watch::Sender<bool>,
    pub gate: crate::gate::Gate,
    pub remote_id: String,
    pub session_started: Arc<AtomicBool>,
    pub detached: Arc<AtomicBool>,
    pub last_connected: Arc<AtomicU64>,
    pub feeds: Feeds,
    /// The participant whose input arrived last: only its soft keyboard rises.
    pub last_input: watch::Sender<Origin>,
    next: AtomicU16,
    s: Mutex<State>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) {
    if let Ok(t) = serde_json::to_string(msg) {
        if out.try_send(t).is_err() {
            warn!("viewers: a participant's link is full or gone — a message was not sent");
        }
    }
}

/// `Custom { kbps }`, the only quality a number can be read off without the compositor.
fn custom_kbps(c: &SessionConfig) -> Option<u32> {
    match c.quality {
        Quality::Custom { bitrate_kbps } => Some(bitrate_kbps),
        _ => None,
    }
}

impl Hub {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        api: Arc<API>,
        track: Arc<TrackLocalStaticSample>,
        audio_track: Arc<TrackLocalStaticSample>,
        cmd_tx: CommandSender,
        input_tx: InputSender,
        listening: watch::Sender<bool>,
        audio_low: watch::Sender<bool>,
        gate: crate::gate::Gate,
        remote_id: String,
        feeds: Feeds,
    ) -> Arc<Self> {
        Arc::new(Self {
            api,
            track,
            audio_track,
            cmd_tx,
            input_tx,
            listening,
            audio_low,
            gate,
            remote_id,
            session_started: Arc::default(),
            detached: Arc::default(),
            last_connected: Arc::new(AtomicU64::new(now_ms())),
            feeds,
            last_input: watch::channel(0).0,
            next: AtomicU16::new(1),
            s: Mutex::default(),
        })
    }

    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        self.s.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// This session's instance, as the relay and the `sessions/` files name it.
    pub fn instance(&self) -> String {
        format!(
            "{}:{}",
            wado_protocol::relay::normalize_remote_id(&self.remote_id),
            crate::instance::instance_key()
        )
    }

    /// A fresh origin for a new peer — the seat's viewer or a room.
    pub fn origin(&self) -> Origin {
        loop {
            let o = self.next.fetch_add(1, Ordering::Relaxed);
            if o != wado_compositor::PRIMARY {
                return o;
            }
        }
    }

    /// A device's link reached this daemon. Not yet a participant.
    pub fn attach(
        &self,
        origin: Origin,
        key: &str,
        name: &str,
        out: mpsc::Sender<String>,
        seat_room: Option<String>,
    ) {
        self.st().peers.insert(
            origin,
            Peer {
                key: key.into(),
                name: name.into(),
                out,
                seat_room,
                pc: None,
                presence: None,
                connected: false,
                audio: false,
                video: false,
                down_since: None,
            },
        );
    }

    /// Its link is gone: out of the session, its input released, its connection closed.
    pub fn detach(&self, origin: Origin) {
        let mut st = self.st();
        let was_member = st.members.has(origin);
        let name = st.peers.get(&origin).map(|p| p.name.clone());
        if let Some(p) = st.peers.remove(&origin) {
            if let Some(pc) = p.pc {
                tokio::spawn(async move {
                    let _ = pc.close().await;
                });
            }
        }
        self.forget(&mut st, origin);
        if was_member {
            self.broadcast(&st, "left", name.as_deref().unwrap_or(""));
        }
        self.after_change(&mut st);
    }

    /// Out of the session (its link stays): membership, signals, dot, held input.
    fn forget(&self, st: &mut State, origin: Origin) {
        st.members.leave(origin);
        st.agg.remove(origin);
        st.board.gone(origin);
        if let Some(p) = st.peers.get_mut(&origin) {
            p.down_since = None;
        }
        let _ = self.input_tx.gone(origin);
    }

    /// Displace a participant: told why and by whom, its connection closed. It does not retry
    /// (I17) — its client offers Rejoin.
    fn displace(&self, st: &mut State, origin: Origin, reason: &str, by: &str) {
        self.forget(st, origin);
        if let Some(p) = st.peers.get_mut(&origin) {
            send(
                &p.out,
                &RelayMsg::SessionLeft {
                    reason: reason.into(),
                    by: by.into(),
                },
            );
            if let Some(room_id) = p.seat_room.clone() {
                send(
                    &p.out,
                    &RelayMsg::SeatHold {
                        room_id,
                        hold_ms: 0,
                    },
                );
            }
            p.connected = false;
            if let Some(pc) = p.pc.take() {
                tokio::spawn(async move {
                    let _ = pc.close().await;
                });
            }
            info!(device = %p.name, reason, by, "viewers: participant displaced");
        }
    }

    /// Everyone's state to the compositor, after any change.
    fn after_change(&self, st: &mut State) {
        let audio = st.peers.values().any(|p| p.connected && p.audio);
        let video = st.peers.values().any(|p| p.connected && p.video);
        // With participants, theirs is the answer. Without (a device on the home page playing
        // the computer's sound, an older flow), any connected viewer — as it always was — and
        // visibility and strain are left to what the viewer itself says.
        if st.agg.is_empty() {
            let _ = self.cmd_tx.send(CompositorCommand::ViewerAttached(video));
        } else {
            let _ = self
                .cmd_tx
                .send(CompositorCommand::ViewerAttached(st.agg.attached()));
            let _ = self
                .cmd_tx
                .send(CompositorCommand::ViewerVisible(st.agg.visible()));
            let strained = st.agg.strained().is_some();
            let _ = self.cmd_tx.send(CompositorCommand::ViewerStrain(strained));
            crate::files::pace::strained(strained);
        }
        let _ = self.listening.send(audio);
        crate::files::pace::video(video);
        if let Some(lead) = st.members.lead_origin() {
            let _ = self.input_tx.lead(lead);
        }
        self.rerate(st);
    }

    /// The shared bitrate moved (someone slow joined or left): rebuild the encoder at it.
    fn rerate(&self, st: &mut State) {
        let Some(mut cfg) = st.lead_config.clone() else {
            return;
        };
        if st.lead_kbps == 0 || !self.session_started.load(Ordering::SeqCst) {
            return;
        }
        let rate = st.agg.rate(st.lead_kbps);
        if rate.kbps == st.applied_kbps {
            return;
        }
        info!(
            kbps = rate.kbps,
            slowed_by = rate.slowed_by.as_deref().unwrap_or("-"),
            floored = rate.floored,
            "viewers: the shared stream's bitrate changes"
        );
        st.applied_kbps = rate.kbps;
        cfg.quality = Quality::Custom {
            bitrate_kbps: rate.kbps,
        };
        let (reply, _) = tokio::sync::oneshot::channel();
        let _ = self
            .cmd_tx
            .send(CompositorCommand::Reconfigure { config: cfg, reply });
    }

    /// Who is in the session, to every participant, with what just happened.
    fn broadcast(&self, st: &State, event: &str, who: &str) {
        let list = st.members.list();
        for m in st.members.members() {
            if let Some(p) = st.peers.get(&m.origin) {
                send(
                    &p.out,
                    &RelayMsg::Participants {
                        list: list.clone(),
                        you: m.origin,
                        event: event.into(),
                        who: who.into(),
                    },
                );
            }
        }
    }

    fn member_of(&self, st: &State, origin: Origin) -> Option<Member> {
        let p = st.peers.get(&origin)?;
        Some(Member::new(origin, &p.key, &p.name, now_ms()))
    }

    /// The cap in force: `server { max-participants }`, lowered by a recent speed test.
    pub fn max_participants(&self) -> u32 {
        let cap = wado_config::live::current().server.max_participants.max(1);
        match self.st().fixed_max {
            Some((m, at)) if at.elapsed() < VERDICT_FOR => cap.min(m.max(1)),
            _ => cap,
        }
    }

    /// A session was started here by `origin`: it leads, alone.
    pub fn start(&self, origin: Origin, config: &SessionConfig) {
        let mut st = self.st();
        let Some(m) = self.member_of(&st, origin) else {
            return;
        };
        let name = m.name.clone();
        for old in st.members.start(m) {
            self.displace(&mut st, old.origin, "ended", &name);
        }
        st.agg.clear();
        st.agg.add(origin, &name);
        st.lead_config = Some(config.clone());
        st.lead_kbps = custom_kbps(config).unwrap_or(0);
        st.applied_kbps = st.lead_kbps;
        if let Some(p) = st.peers.get_mut(&origin) {
            p.down_since = Some(Instant::now());
        }
        let _ = self.input_tx.lead(origin);
        self.broadcast(&st, "started", &name);
    }

    /// The encoder the compositor opened, after a start or reconfigure. When the lead asked for
    /// it (not a rate lowered for someone), its bitrate is the lead's own — what a quality word
    /// like `balanced` came to.
    pub fn note_session(&self, info: &SessionInfo, lead: bool) {
        let mut st = self.st();
        st.encoder = match info.encoder.mode {
            wado_protocol::EncoderMode::Hardware => "hardware",
            wado_protocol::EncoderMode::Software => "software",
        }
        .into();
        let kbps = info.encoder.bitrate_kbps;
        if lead && kbps > 0 && st.applied_kbps == st.lead_kbps {
            st.lead_kbps = kbps;
            st.applied_kbps = kbps;
        }
    }

    /// The running session's encoder: `hardware`, `software`, or empty.
    pub fn encoder(&self) -> String {
        let st = self.st();
        if self.session_started.load(Ordering::SeqCst) {
            st.encoder.clone()
        } else {
            String::new()
        }
    }

    /// `origin` joins the running session. `checked`: the `join` grant and the speed test
    /// apply (a joined room); the seat's own rejoin is today's resume and skips them.
    pub fn join(&self, origin: Origin, checked: bool) -> Result<(), String> {
        let cap = self.max_participants() as usize;
        let mut st = self.st();
        let Some(m) = self.member_of(&st, origin) else {
            return Err("this link is not a viewer".into());
        };
        if st.members.has(origin) {
            return Ok(());
        }
        if checked {
            let g = self.gate.grants(&m.key);
            if !g.join {
                return Err(format!(
                    "this device may not join sessions other devices are in — on the computer: \
                     wado allow {} join",
                    crate::gate::Gate::typed(&m.name)
                ));
            }
            if let Some((Intent::Join, v, at)) = st.verdicts.get(&m.key)
                && at.elapsed() < VERDICT_FOR
                && v.refused()
            {
                return Err(format!(
                    "the speed test says this would not work: {}",
                    v.reasons.first().cloned().unwrap_or_default()
                ));
            }
        }
        let name = m.name.clone();
        let replaced = st.members.join(m, cap).map_err(|r| r.message())?;
        if let Some(old) = replaced {
            self.displace(&mut st, old.origin, "opened_here_again", &name);
        }
        st.agg.add(origin, &name);
        if let Some(p) = st.peers.get_mut(&origin) {
            p.down_since = Some(Instant::now());
        }
        info!(device = %name, participants = st.members.len(), "viewers: joined the session");
        self.broadcast(&st, "joined", &name);
        self.after_change(&mut st);
        // The joiner's decoder needs a picture to start from; everyone gets it (coalesced).
        drop(st);
        self.want_keyframe();
        Ok(())
    }

    /// `origin` takes the session over: it leads, everyone else is disconnected. Returns the
    /// config to rebuild the output with (the taker's size — invariant 8, a fresh `Output`).
    pub fn takeover(
        &self,
        origin: Origin,
        config: &SessionConfig,
    ) -> Result<SessionConfig, String> {
        let mut st = self.st();
        let Some(m) = self.member_of(&st, origin) else {
            return Err("this link is not a viewer".into());
        };
        let g = self.gate.grants(&m.key);
        if !g.takeover {
            return Err(format!(
                "this device may not take sessions over — on the computer: wado allow {} takeover",
                crate::gate::Gate::typed(&m.name)
            ));
        }
        let name = m.name.clone();
        let gone = st.members.takeover(m, now_ms()).map_err(|r| r.message())?;
        for old in gone {
            self.displace(&mut st, old.origin, "taken_over", &name);
        }
        st.agg.clear();
        st.agg.add(origin, &name);
        st.lead_config = Some(config.clone());
        st.lead_kbps = custom_kbps(config).unwrap_or(0);
        st.applied_kbps = st.lead_kbps;
        if let Some(p) = st.peers.get_mut(&origin) {
            p.down_since = Some(Instant::now());
        }
        info!(device = %name, "viewers: session taken over");
        self.broadcast(&st, "taken_over", &name);
        self.after_change(&mut st);
        Ok(config.clone())
    }

    /// `origin` leaves; the session goes on. `true` when nobody is left in it.
    pub fn leave(&self, origin: Origin) -> bool {
        let mut st = self.st();
        if !st.members.has(origin) {
            return st.members.is_empty();
        }
        let name = st
            .peers
            .get(&origin)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        self.forget(&mut st, origin);
        if let Some(p) = st.peers.get_mut(&origin) {
            p.connected = false;
            if let Some(pc) = p.pc.take() {
                tokio::spawn(async move {
                    let _ = pc.close().await;
                });
            }
        }
        self.broadcast(&st, "left", &name);
        self.after_change(&mut st);
        st.members.is_empty()
    }

    /// The session ended — by `by` (any participant may, the user's decision), or by nobody
    /// (the watchdog, a crash). Everyone else is told who.
    pub fn end(&self, by: Option<Origin>) {
        let mut st = self.st();
        let by_name = by
            .and_then(|o| st.peers.get(&o).map(|p| p.name.clone()))
            .unwrap_or_default();
        let others: Vec<Origin> = st
            .members
            .members()
            .iter()
            .map(|m| m.origin)
            .filter(|o| Some(*o) != by)
            .collect();
        for o in others {
            self.displace(&mut st, o, "ended", &by_name);
        }
        st.members.end();
        st.agg.clear();
        st.lead_config = None;
        st.lead_kbps = 0;
        st.applied_kbps = 0;
    }

    /// Any participant's (or the seat viewer's) peer connection is up.
    pub fn any_connected(&self) -> bool {
        self.st().peers.values().any(|p| p.connected)
    }

    pub fn is_member(&self, origin: Origin) -> bool {
        self.st().members.has(origin)
    }

    pub fn members(&self) -> usize {
        self.st().members.len()
    }

    pub fn is_lead(&self, origin: Origin) -> bool {
        let st = self.st();
        st.members.is_lead(origin) || st.members.is_empty()
    }

    /// A reconfigure from `origin`. The lead's reshapes the session; anyone else's is read as the
    /// bitrate it can take (its auto-bitrate cap) and nothing more — the output stays the lead's.
    /// `None`: nothing for the compositor to do.
    pub fn reconfigure(&self, origin: Origin, config: &SessionConfig) -> Option<SessionConfig> {
        let mut st = self.st();
        let lead = st.members.is_lead(origin) || st.members.is_empty();
        if lead {
            st.lead_config = Some(config.clone());
            if let Some(k) = custom_kbps(config) {
                st.lead_kbps = k;
            }
            let mut out = config.clone();
            let rate = st.agg.rate(st.lead_kbps);
            if st.lead_kbps > 0 && rate.kbps < st.lead_kbps {
                out.quality = Quality::Custom {
                    bitrate_kbps: rate.kbps,
                };
            }
            st.applied_kbps = custom_kbps(&out).unwrap_or(st.lead_kbps);
            return Some(out);
        }
        if !st.members.has(origin) {
            return None;
        }
        st.agg.set_cap(origin, custom_kbps(config));
        let before = st.applied_kbps;
        self.rerate(&mut st);
        (st.applied_kbps != before).then(|| {
            let mut c = st.lead_config.clone().unwrap_or_else(|| config.clone());
            c.quality = Quality::Custom {
                bitrate_kbps: st.applied_kbps,
            };
            c
        })
    }

    pub fn set_pc(&self, origin: Origin, pc: Arc<RTCPeerConnection>) {
        if let Some(p) = self.st().peers.get_mut(&origin) {
            p.pc = Some(pc);
        }
    }

    pub fn set_presence_channel(&self, origin: Origin, dc: Arc<RTCDataChannel>) {
        if let Some(p) = self.st().peers.get_mut(&origin) {
            p.presence = Some(dc);
        }
    }

    /// A participant's peer connection came up, or went down.
    pub fn pc_state(&self, origin: Origin, up: bool, audio: bool, video: bool) {
        let mut st = self.st();
        if let Some(p) = st.peers.get_mut(&origin) {
            p.connected = up;
            p.audio = audio;
            p.video = video;
            p.down_since = if up {
                None
            } else {
                p.down_since.or(Some(Instant::now()))
            };
        }
        st.agg.set_attached(origin, up);
        if up {
            self.last_connected.store(now_ms(), Ordering::Relaxed);
        }
        self.after_change(&mut st);
        drop(st);
        if up {
            self.want_keyframe();
        }
    }

    pub fn visible(&self, origin: Origin, on: bool) {
        let mut st = self.st();
        st.agg.set_visible(origin, on);
        let v = if st.agg.has(origin) {
            st.agg.visible()
        } else {
            on
        };
        let _ = self.cmd_tx.send(CompositorCommand::ViewerVisible(v));
    }

    pub fn strain(&self, origin: Origin, on: bool) {
        let mut st = self.st();
        st.agg.set_strained(origin, on);
        let who = if st.agg.has(origin) {
            st.agg.strained().map(str::to_string)
        } else {
            on.then(String::new)
        };
        if let Some(w) = &who
            && st.members.len() > 1
        {
            info!(device = %w, "viewers: a participant's decoder is saturated — shedding for everyone");
        }
        let _ = self
            .cmd_tx
            .send(CompositorCommand::ViewerStrain(who.is_some()));
        crate::files::pace::strained(who.is_some());
    }

    /// A keyframe is wanted (a join, a PLI): forced now, or once the gap since the last ends.
    pub fn want_keyframe(&self) {
        if self.st().agg.want_idr(now_ms()) {
            let _ = self.cmd_tx.send(CompositorCommand::ForceKeyframe);
        }
    }

    /// One input event from `origin`: to the compositor tagged, and to everyone's dots.
    pub fn input(&self, origin: Origin, ev: InputEvent) {
        {
            let mut st = self.st();
            // Once several devices are in, only they drive it; a link that is not (left,
            // displaced) is not let back in by a stray packet.
            if !st.members.is_empty() && !st.members.has(origin) {
                return;
            }
            if let Some((x, y, kind)) = presence::spot(&ev)
                && st.members.len() > 1
            {
                let (name, key) = st
                    .peers
                    .get(&origin)
                    .map(|p| (p.name.clone(), p.key.clone()))
                    .unwrap_or_default();
                st.board.point(origin, &name, hue(&key), x, y, kind);
            }
        }
        let _ = self.input_tx.send_from(origin, ev);
        self.last_input.send_if_modified(|o| {
            let changed = *o != origin;
            *o = origin;
            changed
        });
    }

    /// Whether `origin`'s soft keyboard should rise for an app wanting text: it was the last to
    /// touch the session (or it is alone in it).
    pub fn keyboard_for(&self, origin: Origin) -> bool {
        self.st().members.len() <= 1 || *self.last_input.borrow() == origin
    }

    /// What a device's speed test said, kept for the join or start it was about.
    pub fn record_verdict(&self, key: &str, intent: Intent, v: &LinkVerdict) {
        let mut st = self.st();
        st.verdicts
            .insert(key.to_string(), (intent, v.clone(), Instant::now()));
        if v.down_kbps > 0 {
            st.fixed_max = Some((v.max_participants, Instant::now()));
        }
    }

    /// The last speed test for `key`, if it was about `intent` and recent.
    pub fn verdict(&self, key: &str, intent: Intent) -> Option<LinkVerdict> {
        let st = self.st();
        match st.verdicts.get(key) {
            Some((i, v, at)) if *i == intent && at.elapsed() < VERDICT_FOR => Some(v.clone()),
            _ => None,
        }
    }

    /// For the `sessions/` file: the lead, everyone in, the cap, and the shared bitrate.
    pub fn summary(&self) -> (Option<String>, Vec<Participant>, u32, u32) {
        let max = self.max_participants();
        let st = self.st();
        (
            st.members.lead_name().map(str::to_string),
            st.members.list(),
            max,
            st.applied_kbps,
        )
    }

    /// The output's shape, as the lead set it: width, height, fps.
    pub fn shape(&self) -> Option<(u32, u32, u32)> {
        self.st()
            .lead_config
            .as_ref()
            .map(|c| (c.width, c.height, c.fps))
    }

    pub fn set_link(&self, origin: Origin, link: String) {
        self.st().members.set_link(origin, link);
    }

    pub fn key_of(&self, origin: Origin) -> Option<(String, String)> {
        self.st()
            .peers
            .get(&origin)
            .map(|p| (p.key.clone(), p.name.clone()))
    }

    /// Run the hub's clock: held keyframes, presence dots, and participants whose connection
    /// stayed down. Spawned once per daemon.
    pub async fn run(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_millis(1000 / presence::HZ));
        loop {
            tick.tick().await;
            let now = now_ms();
            let mut sends: Vec<(Arc<RTCDataChannel>, String)> = Vec::new();
            {
                let mut st = self.st();
                if st.agg.idr_due(now) {
                    let _ = self.cmd_tx.send(CompositorCommand::ForceKeyframe);
                }
                let batch = st.board.due(now);
                if !batch.is_empty() {
                    for m in st.members.members() {
                        let Some(dc) = st.peers.get(&m.origin).and_then(|p| p.presence.clone())
                        else {
                            continue;
                        };
                        let mine = presence::for_viewer(&batch, m.origin);
                        if mine.is_empty() {
                            continue;
                        }
                        if let Ok(t) = serde_json::to_string(&serde_json::json!({ "p": mine })) {
                            sends.push((dc, t));
                        }
                    }
                }
                // A participant whose connection has been down too long is out; the others are
                // told. The seat's own viewer is the 30-minute watchdog's, as before.
                let stale: Vec<Origin> = st
                    .members
                    .members()
                    .iter()
                    .filter(|m| {
                        st.peers.get(&m.origin).is_some_and(|p| {
                            p.seat_room.is_none()
                                && p.down_since.is_some_and(|d| d.elapsed() > PARTICIPANT_GONE)
                        })
                    })
                    .map(|m| m.origin)
                    .collect();
                for o in stale {
                    let name = st.peers.get(&o).map(|p| p.name.clone()).unwrap_or_default();
                    warn!(device = %name, "viewers: a participant's connection stayed down — dropped from the session");
                    self.displace(&mut st, o, "stale", "");
                    self.broadcast(&st, "left", &name);
                    self.after_change(&mut st);
                }
            }
            for (dc, t) in sends {
                tokio::spawn(async move {
                    let _ = dc.send_text(t).await;
                });
            }
        }
    }
}
