//! The media player, through MPRIS (`host::media`): play/pause, next and previous track, on the
//! player that is playing — else the first one open.

use wado_protocol::host::MediaOp;

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    [
        ("play-pause", "Play / pause", "play", MediaOp::PlayPause),
        ("next-track", "Next track", "next", MediaOp::Next),
        ("prev-track", "Previous track", "prev", MediaOp::Previous),
    ]
    .into_iter()
    .map(|(id, name, icon, op)| {
        Action::new(id, name, "Media", icon, vec![Need::Mpris], Does::Media(op))
    })
    .collect()
}

pub async fn run(op: MediaOp) -> Result<(), String> {
    // Playing ones come first.
    let players = crate::host::media::state(&[]).await;
    let player = players
        .first()
        .ok_or("no media player is open on this computer")?;
    crate::host::media::act(&player.bus, op).await
}
