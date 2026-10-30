//! The relay socket, behind a trait so the link can be driven by an in-memory pair in tests.
//! The real one is tokio-tungstenite with rustls and compiled-in webpki roots — the daemon's own
//! stack, so no platform verifier is needed on a phone.

use std::io;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

pub use wado_protocol::relay_wire::LEAVE_CLOSE_CODE as LEAVE;

#[async_trait]
pub trait Transport: Send {
    async fn send(&mut self, text: String) -> io::Result<()>;
    /// The next text frame; `None` once the socket is closed. Cancel-safe.
    async fn recv(&mut self) -> Option<io::Result<String>>;
    async fn close(&mut self, code: u16);
}

/// Opens transports: one per dial.
#[async_trait]
pub trait Connector: Send + Sync {
    async fn connect(&self, url: &str) -> io::Result<Box<dyn Transport>>;
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub struct WsTransport {
    ws: Ws,
}

fn other(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

impl WsTransport {
    pub async fn connect(url: &str) -> io::Result<WsTransport> {
        // rustls needs one crypto provider; this build carries only ring's.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (ws, _) = tokio_tungstenite::connect_async(url).await.map_err(other)?;
        Ok(WsTransport { ws })
    }
}

#[async_trait]
impl Transport for WsTransport {
    async fn send(&mut self, text: String) -> io::Result<()> {
        self.ws.send(Message::Text(text)).await.map_err(other)
    }

    async fn recv(&mut self) -> Option<io::Result<String>> {
        loop {
            match self.ws.next().await? {
                Ok(Message::Text(t)) => return Some(Ok(t)),
                Ok(Message::Close(_)) => return None,
                // Pings are answered by tungstenite; binary frames are not the relay's.
                Ok(_) => continue,
                Err(e) => return Some(Err(other(e))),
            }
        }
    }

    async fn close(&mut self, code: u16) {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: "leave".into(),
        };
        let _ = self.ws.close(Some(frame)).await;
    }
}

/// Dials real sockets.
pub struct WsConnector;

#[async_trait]
impl Connector for WsConnector {
    async fn connect(&self, url: &str) -> io::Result<Box<dyn Transport>> {
        Ok(Box::new(WsTransport::connect(url).await?))
    }
}

enum Mem {
    Text(String),
    Close(u16),
}

/// One end of an in-memory socket pair.
pub struct MemTransport {
    tx: mpsc::UnboundedSender<Mem>,
    rx: mpsc::UnboundedReceiver<Mem>,
    /// The close code the other end sent, once it has.
    pub closed_by_peer: Arc<Mutex<Option<u16>>>,
    done: bool,
}

impl MemTransport {
    pub fn pair() -> (MemTransport, MemTransport) {
        let (a_tx, b_rx) = mpsc::unbounded_channel();
        let (b_tx, a_rx) = mpsc::unbounded_channel();
        let end = |tx, rx| MemTransport {
            tx,
            rx,
            closed_by_peer: Arc::default(),
            done: false,
        };
        (end(a_tx, a_rx), end(b_tx, b_rx))
    }
}

#[async_trait]
impl Transport for MemTransport {
    async fn send(&mut self, text: String) -> io::Result<()> {
        self.tx
            .send(Mem::Text(text))
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
    }

    async fn recv(&mut self) -> Option<io::Result<String>> {
        if self.done {
            return None;
        }
        match self.rx.recv().await {
            Some(Mem::Text(t)) => Some(Ok(t)),
            Some(Mem::Close(code)) => {
                *self.closed_by_peer.lock().unwrap() = Some(code);
                self.done = true;
                None
            }
            None => {
                self.done = true;
                None
            }
        }
    }

    async fn close(&mut self, code: u16) {
        let _ = self.tx.send(Mem::Close(code));
        self.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mem_transport_roundtrip() {
        let (mut a, mut b) = MemTransport::pair();
        a.send("hello".into()).await.unwrap();
        assert_eq!(b.recv().await.unwrap().unwrap(), "hello");
        b.send("back".into()).await.unwrap();
        assert_eq!(a.recv().await.unwrap().unwrap(), "back");
        a.close(LEAVE).await;
        assert!(b.recv().await.is_none());
        assert_eq!(*b.closed_by_peer.lock().unwrap(), Some(4001));
    }
}
