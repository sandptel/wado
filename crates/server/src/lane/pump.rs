//! The loop between a link's channel and its socket: pull what producers send, let the
//! [`Outbox`] order and pace it, hand each frame to `send` (which seals and writes). Ends when
//! `send` says the link is gone, or the channel closes and everything queued has gone.

use std::future::Future;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::outbox::{Next, Outbox};

/// A socket that has taken no frame for this long is a dead link: the relay has stopped reading
/// it, and its TCP may not say so for a quarter of an hour. As long as the relay's own silence
/// timeout, so a live but slow relay is never cut off first by this side.
pub const WRITE_STALL: Duration = Duration::from_secs(45);

/// `generation`: which device the link serves now (`crate::e2e::link::Link::generation`);
/// asked before every frame is taken or handed out, so nothing queued for one device is sealed
/// for the next (`Outbox::set_generation`).
///
/// The channel is read **always** — while bulk waits for the budget, and while the socket is
/// taking a frame — so a producer's send never waits on the lane, and the relay loop sending its
/// `pong` never waits behind a picture. What bounds the queue is the [`Outbox`]: bulk past its
/// stop is dropped and said so; urgent frames are small. A write that does not finish in
/// [`WRITE_STALL`] ends the link.
pub async fn run<F, Fut, G>(
    mut rx: mpsc::Receiver<String>,
    mut outbox: Outbox,
    generation: G,
    mut send: F,
) where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = bool>,
    G: Fn() -> u64,
{
    let mut open = true;
    let mut seq: u64 = 0;
    let take = |outbox: &mut Outbox, open: &mut bool, r: Option<String>| match r {
        Some(t) => {
            outbox.set_generation(generation());
            outbox.push(t)
        }
        None => *open = false,
    };
    loop {
        if outbox.is_empty() {
            if !open {
                return;
            }
            let r = rx.recv().await;
            take(&mut outbox, &mut open, r);
            if !open {
                return;
            }
        }
        // Take whatever else is already waiting, so an answer queued behind a burst of output is
        // seen at once. Streaming producers hold themselves back on the gauge; the outbox's own
        // stop is only a safety net for one that does not.
        while open {
            match rx.try_recv() {
                Ok(t) => take(&mut outbox, &mut open, Some(t)),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => open = false,
            }
        }
        outbox.set_generation(generation());
        match outbox.next() {
            Next::Send(t) => {
                // Timed (`crate::slow`): how long the frame sat in the lane, and how long the
                // socket took it. A device's "didn't answer" is one of these, or the handler.
                seq += 1;
                let (age, depth) = (outbox.last_age, outbox.depth());
                let kind = crate::e2e::link::kind(&t).unwrap_or("?").to_owned();
                let t0 = std::time::Instant::now();
                let write = tokio::time::timeout(WRITE_STALL, send(t));
                tokio::pin!(write);
                let sent = loop {
                    tokio::select! {
                        biased;
                        r = &mut write => break r,
                        r = rx.recv(), if open => take(&mut outbox, &mut open, r),
                    }
                };
                match sent {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(_) => {
                        warn!(
                            "lane: the socket took no {kind} frame for {WRITE_STALL:?} — the link is gone"
                        );
                        return;
                    }
                }
                let write = t0.elapsed();
                crate::slow::report(
                    "lane",
                    age + write,
                    &kind,
                    seq,
                    format_args!(
                        "(queued {} ms; {} urgent, {} bulk / {} KB still waiting; socket write {} ms)",
                        age.as_millis(),
                        depth.0,
                        depth.1,
                        depth.2 / 1024,
                        write.as_millis()
                    ),
                );
            }
            Next::Wait(d) => {
                // While bulk waits for the budget, anything urgent that arrives still goes now.
                tokio::select! {
                    _ = tokio::time::sleep(d) => {}
                    r = rx.recv(), if open => take(&mut outbox, &mut open, r),
                }
            }
            Next::Idle => {}
        }
        if outbox.dropped >= 100 {
            debug!(
                "lane: {} log lines not sent — the link was busy",
                outbox.dropped
            );
            outbox.dropped = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use wado_protocol::lane::Pace;

    #[tokio::test(start_paused = true)]
    async fn a_flood_is_paced_and_an_answer_still_goes_at_once() {
        let (tx, rx) = mpsc::channel(8);
        let sent: Arc<Mutex<Vec<(tokio::time::Instant, String)>>> = Arc::default();
        let log = Arc::clone(&sent);
        let t0 = tokio::time::Instant::now();
        let outbox = Outbox::new(Pace::new(64_000, 64_000));
        let gauge = outbox.gauge();
        let pump = tokio::spawn(run(
            rx,
            outbox,
            || 1,
            move |t| {
                log.lock().unwrap().push((tokio::time::Instant::now(), t));
                async { true }
            },
        ));
        let chunk = format!(r#"{{"type":"pty_output","data":"{}"}}"#, "x".repeat(24_000));
        // A streaming producer, as the shells' and the files' are: it waits for room first.
        let producer = {
            let tx = tx.clone();
            tokio::spawn(async move {
                for _ in 0..20 {
                    gauge.room().await;
                    tx.send(chunk.clone()).await.unwrap();
                }
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        tx.send(r#"{"type":"sdp_answer","sdp":"x"}"#.into())
            .await
            .unwrap();
        producer.await.unwrap();
        drop(tx);
        pump.await.unwrap();
        let sent = sent.lock().unwrap();
        let answer = sent.iter().find(|(_, t)| t.contains("sdp_answer")).unwrap();
        let late = answer.0 - t0 - std::time::Duration::from_millis(500);
        assert!(
            late < std::time::Duration::from_millis(1600),
            "the answer waited {late:?}"
        );
        // 20 × ~32 KB on the wire at 64 KB/s after a 64 KB burst: about nine seconds.
        let span = sent.last().unwrap().0 - t0;
        assert!(
            span > std::time::Duration::from_secs(8),
            "not paced: {span:?}"
        );
        assert_eq!(sent.len(), 21);
    }

    /// The hang of 2026-10-30 17:30Z (HANDOVER item 0): 31 clipboard pictures (16 MB of
    /// `clip_data`) queued on the lane, the relay stopped taking the socket's writes, and the
    /// relay loop — blocked sending its `pong` into a full channel — never read again, so its
    /// own silence timeout could not fire.
    #[tokio::test(start_paused = true)]
    async fn a_stuck_socket_never_stalls_the_reader() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (out_tx, rx) = mpsc::channel::<String>(128);
        // A relay socket that takes 64 KB/s for its first megabyte, then stops reading.
        let taken = Arc::new(AtomicUsize::new(0));
        let pump = tokio::spawn(run(
            rx,
            Outbox::new(Pace::new(1 << 30, 1 << 30)),
            || 1,
            move |t| {
                let before = taken.fetch_add(t.len(), Ordering::SeqCst);
                async move {
                    if before >= 1 << 20 {
                        std::future::pending::<()>().await;
                    }
                    tokio::time::sleep(Duration::from_secs_f64(t.len() as f64 / 65_536.0)).await;
                    true
                }
            },
        ));
        // Pictures fetched for the Clipboard surface's tiles, each answer from its own task…
        let picture = format!(
            r#"{{"type":"clip_data","id":"1","data":"{}"}}"#,
            "x".repeat(560_000)
        );
        for _ in 0..31 {
            let (tx, p) = (out_tx.clone(), picture.clone());
            tokio::spawn(async move { tx.send(p).await });
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        // …and the session's small answers behind them.
        for _ in 0..200 {
            let tx = out_tx.clone();
            tokio::spawn(async move { tx.send(r#"{"type":"pad_out","msg":{}}"#.into()).await });
        }
        // The relay loop: a ping every 15 s, answered before the next frame is read.
        for n in 0..4 {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let pong = out_tx.send(r#"{"type":"pong"}"#.into());
            assert!(
                tokio::time::timeout(Duration::from_secs(1), pong)
                    .await
                    .is_ok(),
                "ping {n}: the reader is stuck behind the lane"
            );
        }
        // A socket that takes nothing for `WRITE_STALL` is a dead link: the writer ends, and the
        // loop sees its channel close.
        tokio::time::sleep(WRITE_STALL).await;
        assert!(
            pump.is_finished(),
            "the writer still waits on a dead socket"
        );
        assert!(out_tx.is_closed());
    }
}
