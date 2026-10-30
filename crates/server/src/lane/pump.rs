//! The loop between a link's channel and its socket: pull what producers send, let the
//! [`Outbox`] order and pace it, hand each frame to `send` (which seals and writes). Ends when
//! `send` says the link is gone, or the channel closes and everything queued has gone.

use std::future::Future;

use tokio::sync::mpsc;
use tracing::debug;

use super::outbox::{Next, Outbox};

pub async fn run<F, Fut>(mut rx: mpsc::Receiver<String>, mut outbox: Outbox, mut send: F)
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = bool>,
{
    let mut open = true;
    loop {
        if outbox.is_empty() {
            if !open {
                return;
            }
            match rx.recv().await {
                Some(t) => outbox.push(t),
                None => return,
            }
        }
        // Take whatever else is already waiting, so an answer queued behind a burst of output
        // is seen — but only while the backlog is small enough to keep backpressure honest.
        while open && outbox.wants_more() {
            match rx.try_recv() {
                Ok(t) => outbox.push(t),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => open = false,
            }
        }
        match outbox.next() {
            Next::Send(t) => {
                if !send(t).await {
                    return;
                }
            }
            Next::Wait(d) => {
                // While bulk waits for the budget, anything urgent that arrives still goes now.
                tokio::select! {
                    _ = tokio::time::sleep(d) => {}
                    r = rx.recv(), if open && outbox.wants_more() => match r {
                        Some(t) => outbox.push(t),
                        None => open = false,
                    },
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
        let pump = tokio::spawn(run(rx, outbox, move |t| {
            log.lock().unwrap().push((tokio::time::Instant::now(), t));
            async { true }
        }));
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
}
