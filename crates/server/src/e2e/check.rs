//! The daemon's side of a typed pairing's check words (`crate::gate::confirm`): hold the link,
//! show the words on the computer, and let the device in or turn it away on the answer.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::envelope::{E2eMsg, sas_words};

use super::handshake::Done;
use super::link::{Link, WAKE};
use crate::gate::{CONFIRM_WAIT, Check, DECLINED, Gate, Grants, NO_ANSWER};

/// The device that proved a typed code, as it will be written to the trust list on a yes.
pub struct Pending {
    pub key: String,
    pub name: String,
    pub pk: String,
    pub grants: Grants,
    /// The typed code, which names the question.
    pub typed: String,
}

/// Start the check: the link waits, the device is sent the nonce for its words, the computer
/// is asked. Returns the frame for the device.
pub fn begin(link: &mut Link, gate: &Gate, p: &Pending, done: Done) -> String {
    let words = sas_words(done.sas).join(" ");
    link.confirming(done.sealer, done.opener);
    gate.ask(&p.typed, &words, &p.name);
    info!(
        "gate: {} typed a code — check words on the computer: {words}",
        p.name
    );
    serde_json::to_string(&E2eMsg::E2eSas {
        nonce: done.sas_nonce,
    })
    .unwrap_or_default()
}

/// Wait for the answer at the computer, then let the device in (pinned) or refuse it. `room` is
/// the seat's current room; the device is gone once it is no longer `rid`.
pub async fn settle(
    gate: Gate,
    link: Arc<Mutex<Link>>,
    room: Arc<Mutex<String>>,
    rid: String,
    out_tx: mpsc::Sender<String>,
    p: Pending,
) {
    let here = || *room.lock().unwrap_or_else(|e| e.into_inner()) == rid;
    let check = gate.await_answer(&p.typed, CONFIRM_WAIT, here).await;
    let out = match check {
        Check::Yes => {
            gate.confirmed(&p.key, &p.name, &p.pk, &p.grants);
            gate.settle(&p.typed, Check::Paired);
            let mut l = link.lock().unwrap_or_else(|e| e.into_inner());
            (here() && l.confirm()).then(|| WAKE.to_string())
        }
        Check::Left => {
            info!("gate: {} left before its check words were answered", p.name);
            gate.settle(&p.typed, Check::Left);
            None
        }
        refused => {
            let (why, settled) = if refused == Check::No {
                (DECLINED, Check::Declined)
            } else {
                (NO_ANSWER, Check::NoAnswer)
            };
            warn!("gate: {} not paired — {why}", p.name);
            gate.settle(&p.typed, settled);
            let mut l = link.lock().unwrap_or_else(|e| e.into_inner());
            (here() && l.is_confirming())
                .then(|| serde_json::to_string(&l.fail(why)).unwrap_or_default())
        }
    };
    if let Some(out) = out {
        out_tx.send(out).await.ok();
    }
}
