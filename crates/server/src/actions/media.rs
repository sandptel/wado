//! The media player, through the platform's media players (`wado_platform::media`): play/pause,
//! next and previous track, on the player that is playing — else the first one open.

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
    let media = crate::platform::get().media().map_err(|a| a.why)?;
    media.fresh(std::time::Duration::ZERO).await;
    // Playing ones come first.
    let bus = media
        .state()
        .borrow()
        .first()
        .map(|p| p.bus.clone())
        .ok_or("no media player is open on this computer")?;
    media.act(bus, op).await
}
