//! The computer's sound as a viewer is shown it, and moving everything to one output — the policy
//! over the platform's [`AudioMixer`].

use std::time::Duration;

use wado_platform::audio::AudioMixer;
use wado_protocol::host::HostAudio;

/// What `phone` (this daemon's "This phone" output, while there is one) sees: another daemon's
/// "This phone" (a pool has several) is someone else's device, not an output this viewer should
/// send sound to; its own is labelled as the phone.
pub fn decorate(a: &mut HostAudio, phone: Option<&str>) {
    a.phone_sink = phone.map(str::to_string);
    a.sinks
        .retain(|s| !s.name.starts_with("wado-") || Some(s.name.as_str()) == phone);
    if let Some(p) = phone {
        for s in &mut a.sinks {
            if s.name == p {
                s.label = "This phone".into();
            }
        }
    }
}

/// The mixer's state now, as `phone` sees it.
pub async fn now(mixer: &dyn AudioMixer, phone: Option<&str>) -> HostAudio {
    mixer.fresh(Duration::ZERO).await;
    let mut a = mixer.state().borrow().clone();
    decorate(&mut a, phone);
    a
}

/// Everything to one output: the default (so what starts next goes there) and every stream now.
/// `sink` must be one of the outputs `phone` is shown.
pub async fn all_to(
    mixer: &dyn AudioMixer,
    sink: String,
    phone: Option<&str>,
) -> Result<(), String> {
    let a = now(mixer, phone).await;
    if !a.sinks.iter().any(|s| s.name == sink) {
        return Err("no such output".into());
    }
    mixer.set_default(sink.clone()).await?;
    for s in &a.streams {
        mixer.move_stream(s.id, sink.clone()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;
    use wado_protocol::host::{Sink, Stream};

    fn sink(name: &str) -> Sink {
        Sink {
            name: name.into(),
            label: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn another_daemons_phone_is_hidden_and_ours_is_named() {
        let mut a = HostAudio {
            sinks: vec![sink("spk"), sink("wado-1-10"), sink("wado-2-20")],
            ..Default::default()
        };
        decorate(&mut a, Some("wado-1-10"));
        let names: Vec<_> = a
            .sinks
            .iter()
            .map(|s| (s.name.as_str(), s.label.as_str()))
            .collect();
        assert_eq!(names, [("spk", "spk"), ("wado-1-10", "This phone")]);
        assert_eq!(a.phone_sink.as_deref(), Some("wado-1-10"));
    }

    #[tokio::test]
    async fn everything_moves_to_the_phone() {
        let f = Fakes::new();
        f.audio.state.will_read(HostAudio {
            sinks: vec![sink("spk"), sink("wado-1-10")],
            streams: vec![
                Stream {
                    id: 7,
                    ..Default::default()
                },
                Stream {
                    id: 9,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        all_to(f.audio.as_ref(), "wado-1-10".into(), Some("wado-1-10"))
            .await
            .unwrap();
        assert_eq!(
            f.log.calls(),
            [
                "audio.set_default wado-1-10",
                "audio.move_stream 7 wado-1-10",
                "audio.move_stream 9 wado-1-10"
            ]
        );
        let other = all_to(f.audio.as_ref(), "wado-2-20".into(), Some("wado-1-10")).await;
        assert_eq!(
            other,
            Err("no such output".into()),
            "another daemon's phone"
        );
    }
}
