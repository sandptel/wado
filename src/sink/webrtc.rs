//! WebRTC video sink.
//!
//! Streams the encoded H.264 Annex-B frames produced by the compositor over
//! WebRTC (DTLS-SRTP). This replaces the old raw-UDP sink, whose one-frame-per-
//! datagram scheme silently dropped >64 KB frames and lost IP fragments under
//! heavy motion (the "pixels mixing" artifact). WebRTC's H.264 payloader does
//! MTU-sized RTP packetization (FU-A) for us, so large keyframes survive.
//!
//! ## Threading
//! webrtc-rs is fully async (tokio); the compositor runs on a synchronous
//! `calloop` loop. We bridge them with a bounded, drop-on-full channel: the
//! render thread calls [`FrameSink::send`] which does a non-blocking `try_send`,
//! and a dedicated tokio runtime on its own thread owns the peer connections and
//! awaits `write_sample`. `send` therefore never blocks the render tick.
//!
//! ## Signaling
//! A minimal built-in HTTP server: `GET /` serves a browser test page and
//! `POST /offer` accepts an SDP offer (`{type,sdp}` JSON) and returns the answer.
//! Non-trickle ICE is used (we wait for `gathering_complete_promise()` before
//! replying), which is sufficient for LAN/localhost.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::{API, APIBuilder};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use super::FrameSink;

/// Boxed send-safe error for the async signaling tasks.
type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Bounded so a slow/absent network never lets encoded frames pile up. On a full
/// channel we drop the newest frame (latest-wins is fine for live video — a
/// periodic IDR resyncs the decoder within `keyframe_interval`).
const FRAME_CHANNEL_CAPACITY: usize = 4;

pub struct WebRtcSink {
    tx: mpsc::Sender<Vec<u8>>,
    /// Kept so the runtime thread's lifetime is tied to the sink; the thread runs
    /// the tokio runtime (signaling server + frame pump).
    _runtime_thread: std::thread::JoinHandle<()>,
}

impl WebRtcSink {
    /// Spawn the WebRTC runtime and bind the signaling server on `http_addr`.
    /// `fps` sets the per-sample duration so RTP timestamps advance correctly.
    pub fn new(http_addr: &str, fps: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(FRAME_CHANNEL_CAPACITY);
        let addr = http_addr.to_string();
        let frame_dur = Duration::from_nanos(1_000_000_000 / fps.max(1) as u64);

        let runtime_thread = std::thread::Builder::new()
            .name("wado-webrtc".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[webrtc] failed to build tokio runtime: {e}");
                        return;
                    }
                };
                rt.block_on(async move {
                    if let Err(e) = run(addr, rx, frame_dur).await {
                        eprintln!("[webrtc] signaling server exited: {e}");
                    }
                });
            })?;

        Ok(Self { tx, _runtime_thread: runtime_thread })
    }
}

impl FrameSink for WebRtcSink {
    fn send(&mut self, nal_data: &[u8]) {
        // Non-blocking: never stall the render tick on the network. Dropped frames
        // (channel full / no viewer yet) are recovered by the next IDR.
        let _ = self.tx.try_send(nal_data.to_vec());
    }
}

/// tokio entry point: build the shared API + track, start the frame pump, and
/// run the HTTP signaling server forever.
async fn run(
    addr: String,
    mut rx: mpsc::Receiver<Vec<u8>>,
    frame_dur: Duration,
) -> Result<(), BoxErr> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;
    let api = Arc::new(
        APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build(),
    );

    // One shared track fed by the encoder. write_sample with no bound senders is a
    // harmless no-op, so the pump can run before any viewer connects, and a single
    // track naturally fans out to multiple viewers.
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability { mime_type: MIME_TYPE_H264.to_owned(), ..Default::default() },
        "video".to_owned(),
        "wado".to_owned(),
    ));

    // Frame pump: encoded frame bytes → write_sample.
    {
        let track = Arc::clone(&track);
        tokio::spawn(async move {
            while let Some(buf) = rx.recv().await {
                let sample =
                    Sample { data: Bytes::from(buf), duration: frame_dur, ..Default::default() };
                if let Err(e) = track.write_sample(&sample).await {
                    eprintln!("[webrtc] write_sample error: {e}");
                }
            }
        });
    }

    let listener = TcpListener::bind(&addr).await?;
    eprintln!("[webrtc] signaling on http://{addr}  — open it in a browser to view the stream");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let api = Arc::clone(&api);
        let track = Arc::clone(&track);
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, api, track).await {
                eprintln!("[webrtc] connection error: {e}");
            }
        });
    }
}

/// Serve one HTTP request: `GET /` → test page, `POST /offer` → SDP answer.
async fn handle_conn(
    mut stream: TcpStream,
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
) -> Result<(), BoxErr> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];

    // Read until the end of the headers.
    let header_end = loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(()); // client closed before sending a full request
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            write_response(&mut stream, "431 Request Header Fields Too Large", "text/plain", b"")
                .await?;
            return Ok(());
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    // Read the body (everything after the header terminator).
    let body_start = header_end + 4;
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/") => {
            let html = include_str!("../../assets/webrtc_test.html");
            write_response(&mut stream, "200 OK", "text/html; charset=utf-8", html.as_bytes())
                .await?;
        }
        ("POST", "/offer") => {
            let offer_json = String::from_utf8_lossy(&body);
            match handle_offer(api, track, &offer_json).await {
                Ok(answer) => {
                    write_response(&mut stream, "200 OK", "application/json", answer.as_bytes())
                        .await?;
                }
                Err(e) => {
                    eprintln!("[webrtc] offer handling failed: {e}");
                    write_response(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain",
                        b"offer failed",
                    )
                    .await?;
                }
            }
        }
        _ => {
            write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await?;
        }
    }

    Ok(())
}

/// Build a peer connection for one viewer, attach the shared video track, and
/// produce the SDP answer (with all ICE candidates already gathered).
async fn handle_offer(
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
    offer_json: &str,
) -> Result<String, BoxErr> {
    let offer: RTCSessionDescription = serde_json::from_str(offer_json)?;

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let pc = Arc::new(api.new_peer_connection(config).await?);

    let rtp_sender =
        pc.add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>).await?;

    // Drain RTCP (required so interceptors run) and, by holding `pc`, keep the
    // connection alive until the sender closes — at which point everything drops.
    let pc_keepalive = Arc::clone(&pc);
    tokio::spawn(async move {
        let _pc = pc_keepalive;
        let mut rtcp_buf = vec![0u8; 1500];
        while rtp_sender.read(&mut rtcp_buf).await.is_ok() {}
        eprintln!("[webrtc] viewer disconnected");
    });

    pc.on_peer_connection_state_change(Box::new(|state| {
        eprintln!("[webrtc] peer connection state: {state}");
        Box::pin(async {})
    }));

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;

    // Gather all candidates before answering (non-trickle).
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;

    let local = pc
        .local_description()
        .await
        .ok_or("no local description after gathering")?;
    Ok(serde_json::to_string(&local)?)
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), BoxErr> {
    let header = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
