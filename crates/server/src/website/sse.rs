//! The `/events` live-log stream — one job: relay wado's tracing output to a listener as
//! Server-Sent Events over a long-lived raw TCP response.

use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::broadcast;

use super::logbus::LogBus;

/// Stream wado's live tracing output to a `/events` listener as Server-Sent Events.
pub(super) async fn serve_sse(mut stream: TcpStream, log_bus: &LogBus) -> crate::Result<()> {
    let header = "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: keep-alive\r\n\r\n";
    stream.write_all(header.as_bytes()).await?;

    // Backfill recent history so a freshly opened panel isn't empty.
    for line in log_bus.backfill() {
        stream
            .write_all(format!("data: {line}\n\n").as_bytes())
            .await?;
    }
    stream.flush().await?;

    let mut rx = log_bus.subscribe();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(line) => {
                    if stream.write_all(format!("data: {line}\n\n").as_bytes()).await.is_err() {
                        break; // client disconnected
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = heartbeat.tick() => {
                // Comment line; also surfaces a dead socket as a write error.
                if stream.write_all(b": keepalive\n\n").await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}
