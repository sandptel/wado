//! Request parsing and route dispatch for the control plane — one job: read one HTTP
//! request off a connection and hand it to the handler for its route.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tracing::{error, warn};
use wado_compositor::CompositorCommand;
use wado_protocol::{SessionControl, SessionInfo};

use super::http::{find_subsequence, write_preflight, write_response};
use super::{ServerCtx, now_ms};

/// Reject oversized request bodies (SDP/config are tiny; this is a DoS guard).
const MAX_BODY_BYTES: usize = 256 * 1024;
/// How long `/session/start` waits for the compositor thread to reply.
const START_REPLY_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn handle_conn(mut stream: TcpStream, ctx: Arc<ServerCtx>) -> crate::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];

    let header_end = loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            write_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain",
                b"",
            )
            .await?;
            return Ok(());
        }
    };

    // Liveness for `viewer_watchdog`: a viewer polls this server constantly (stats, events,
    // timings), so silence here means nobody is on the other end.
    ctx.last_request.store(now_ms(), Ordering::Relaxed);

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
    if content_length > MAX_BODY_BYTES {
        write_response(
            &mut stream,
            "413 Payload Too Large",
            "text/plain",
            b"body too large",
        )
        .await?;
        return Ok(());
    }

    // CORS preflight: the client is a separate origin, so browsers preflight the
    // JSON POSTs. Answer any OPTIONS with the allowed methods/headers. No body.
    if method == "OPTIONS" {
        return write_preflight(&mut stream).await;
    }

    // Live log stream is a long-lived response — handle before the normal path.
    if method == "GET" && path == "/events" {
        return super::sse::serve_sse(stream, &ctx.log_bus).await;
    }

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
        // Per-stage pipeline timings for the client's latency breakdown. A plain polled
        // GET rather than a new SSE event type: /events is a hand-rolled raw-TCP log
        // stream with no named-event support, and telemetry the client samples once a
        // second does not justify building that out.
        // No session required: you pick what to launch before there is anything to launch
        // it into. Scanned per request rather than cached — a package can be installed while
        // the server is running, and the scan is a few milliseconds of directory reads.
        ("GET", "/apps") => {
            let mut apps = crate::apps::discover();
            // Marked here rather than in `discover`, because "what is installed" and "what is
            // running" come from different places and only one of them needs the compositor.
            crate::apps::running::mark(&mut apps, &ctx.cmd_tx).await;
            let body = serde_json::to_vec(&apps).unwrap_or_else(|_| b"[]".to_vec());
            write_response(&mut stream, "200 OK", "application/json", &body).await?;
        }
        ("GET", "/timing") => {
            let t = *ctx.timings.borrow();
            let queue_ms = ctx.queue_us.load(Ordering::Relaxed) as f64 / 1e3;
            let body = serde_json::json!({
                "capture_ms": t.capture_ms,
                "encode_ms": t.encode_ms,
                "queue_ms": queue_ms,
                "tick_ms": t.tick_ms,
                "fps": t.fps,
                "dropped": t.dropped,
            });
            write_response(
                &mut stream,
                "200 OK",
                "application/json",
                body.to_string().as_bytes(),
            )
            .await?;
        }
        ("GET", "/") => {
            // No UI here anymore — the client is the separate `wado-client` app.
            let msg = b"wado control server (API only). Run the wado-client app to connect.";
            write_response(&mut stream, "200 OK", "text/plain", msg).await?;
        }
        ("POST", "/session/start") => match handle_session_start(&ctx, &body).await {
            Ok(info) => {
                ctx.session_started.store(true, Ordering::SeqCst);
                let body = serde_json::to_string(&info).unwrap_or_else(|_| "{}".into());
                write_response(&mut stream, "200 OK", "application/json", body.as_bytes()).await?
            }
            Err(e) => {
                warn!("session start rejected: {e}");
                write_response(&mut stream, "409 Conflict", "text/plain", e.as_bytes()).await?
            }
        },
        ("POST", "/session/stop") => {
            ctx.session_started.store(false, Ordering::SeqCst);
            let _ = ctx.cmd_tx.send(CompositorCommand::Stop);
            write_response(&mut stream, "200 OK", "text/plain", b"stopped").await?;
        }
        // One route for every verb against a running session. `/session/launch` was its
        // predecessor; adding four window actions in that shape would have meant four more
        // routes here and four more relay messages, so the verb moved into the body.
        ("POST", "/session/control") => match serde_json::from_slice::<SessionControl>(&body) {
            Ok(SessionControl::Launch { command }) if command.trim().is_empty() => {
                write_response(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    b"empty command",
                )
                .await?
            }
            Ok(SessionControl::Launch { command }) => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Launch { command });
                write_response(&mut stream, "200 OK", "text/plain", b"launched").await?;
            }
            Ok(SessionControl::Window(action)) => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Window(action));
                write_response(&mut stream, "200 OK", "text/plain", b"ok").await?;
            }
            Err(e) => {
                warn!("control rejected: {e}");
                write_response(&mut stream, "400 Bad Request", "text/plain", b"bad control").await?
            }
        },
        ("POST", "/offer") => {
            let offer_json = String::from_utf8_lossy(&body);
            match super::offer::handle_offer(&ctx, &offer_json).await {
                Ok(answer) => {
                    write_response(&mut stream, "200 OK", "application/json", answer.as_bytes())
                        .await?
                }
                Err(e) => {
                    error!("offer handling failed: {e}");
                    write_response(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain",
                        b"offer failed",
                    )
                    .await?
                }
            }
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await?,
    }

    Ok(())
}

/// Parse a `SessionConfig` and ask the compositor thread to start a session,
/// bounded by a timeout so a wedged compositor can't hang the HTTP connection.
async fn handle_session_start(
    ctx: &ServerCtx,
    body: &[u8],
) -> std::result::Result<SessionInfo, String> {
    let config = serde_json::from_slice(body).map_err(|e| format!("bad config: {e}"))?;
    let (reply_tx, reply_rx) = oneshot::channel();
    ctx.cmd_tx
        .send(CompositorCommand::Start {
            config,
            reply: reply_tx,
        })
        .map_err(|_| "compositor unavailable".to_string())?;
    match tokio::time::timeout(START_REPLY_TIMEOUT, reply_rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("compositor dropped reply".into()),
        Err(_) => Err("session start timed out".into()),
    }
}
