//! Which part of the trip is slow: the computer, the network or the phone. Read off the latency
//! legs, for the line over the picture, so a tester sees the culprit, not seven numbers.
//!
//! Legs are only added up within one clock: the server's (capture + encode + queue) and the
//! browser's (buffer + decode). The network leg is half the round trip. The three are compared,
//! never summed; that would invent a glass-to-glass figure the clocks cannot support.

/// `(side, text)`: `side` is `computer`, `network`, `phone` or `audio`, for the colour.
///
/// `audio` is its own side because A/V sync is mandatory (Decision Log 2026-10-03): when most of
/// the playout buffer is the minimum delay Chrome imposes to keep video level with sound
/// (`sync`), the fix is in the audio path, not the video one.
pub fn blame(stages: &[(String, f64)], decoder: &str, fps: u32) -> Option<(&'static str, String)> {
    let get = |k: &str| stages.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
    if let (Some(sync), Some(buf)) = (get("sync"), get("buf")) {
        if sync > 40.0 && sync >= 0.6 * buf {
            let audio =
                get("audio").map_or(String::new(), |a| format!(" (audio buffer {a:.0} ms)"));
            return Some((
                "audio",
                format!(
                    "slowest: audio sync — video held {sync:.0} ms to stay level with sound{audio}"
                ),
            ));
        }
    }
    let leg = |ks: &[&'static str]| -> (f64, &'static str, f64) {
        let mut sum = 0.0;
        let (mut top, mut top_v) = ("", -1.0);
        for k in ks {
            if let Some(v) = get(k) {
                sum += v;
                if v > top_v {
                    (top, top_v) = (*k, v);
                }
            }
        }
        (sum, top, top_v)
    };
    let frame = 1000.0 / fps.max(1) as f64;
    // The playout buffer is the browser smoothing uneven *arrival*: the network's doing, unless
    // the decoder is the one falling behind — then frames queue for it and it is the phone's.
    let decoder_slow = get("decode").is_some_and(|d| d > 2.0 * frame);
    let (net_legs, phone_legs): (&[&'static str], &[&'static str]) = if decoder_slow {
        (&["net"], &["buf", "decode"])
    } else {
        (&["net", "buf"], &["decode"])
    };
    let sides = [
        ("computer", leg(&["capture", "encode", "queue"])),
        ("network", leg(net_legs)),
        ("phone", leg(phone_legs)),
    ];
    let (side, (total, top, top_v)) = sides
        .into_iter()
        .filter(|(_, (t, _, _))| *t > 0.0)
        .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
    let software = ["FFmpeg", "libavcodec", "OpenH264", "powerEfficient=false"]
        .iter()
        .any(|s| decoder.contains(s));
    let why = match (side, top) {
        ("phone", "decode") if software => " — software decoder; lower fps or resolution",
        ("phone", "decode") if top_v > 3.0 * frame => " — the phone can't keep up; lower fps",
        ("phone", "buf") => " — frames queue for a decoder that can't keep up; lower fps",
        ("network", "buf") => {
            " — uneven arrival, buffered to smooth it; the link can't carry this bitrate"
        }
        ("network", _) if total > 60.0 => " — long route; try a closer relay or Wi-Fi",
        ("computer", "encode") if top_v > frame => " — the computer's encoder is busy",
        ("computer", "queue") if top_v > frame => " — frames waiting to send",
        _ => "",
    };
    let name = match top {
        "net" => "round trip/2",
        "buf" => "buffer",
        other => other,
    };
    Some((side, format!("slowest: {side} — {name} {top_v:.0} ms{why}")))
}

#[cfg(test)]
mod tests {
    use super::blame;

    fn s(v: &[(&str, f64)]) -> Vec<(String, f64)> {
        v.iter().map(|(k, x)| (k.to_string(), *x)).collect()
    }

    #[test]
    fn names_the_slowest_side_and_why() {
        let legs = s(&[
            ("capture", 0.3),
            ("encode", 2.0),
            ("queue", 0.5),
            ("net", 20.0),
            ("buf", 18.0),
            ("decode", 150.0),
        ]);
        let (side, text) = blame(
            &legs,
            "ExternalDecoder (MediaCodec) powerEfficient=true",
            60,
        )
        .unwrap();
        assert_eq!(side, "phone");
        assert!(
            text.contains("decode 150 ms") && text.contains("can't keep up"),
            "{text}"
        );
        let (_, text) = blame(&legs, "libavcodec powerEfficient=false", 60).unwrap();
        assert!(text.contains("software decoder"), "{text}");
        let (side, _) = blame(
            &s(&[("encode", 3.0), ("net", 90.0), ("decode", 9.0)]),
            "",
            60,
        )
        .unwrap();
        assert_eq!(side, "network");
        assert!(blame(&[], "", 60).is_none());
        // Most of the buffer is the sync floor: the audio path is the culprit.
        let (side, text) = blame(
            &s(&[
                ("net", 20.0),
                ("buf", 300.0),
                ("decode", 9.0),
                ("audio", 280.0),
                ("sync", 290.0),
            ]),
            "",
            60,
        )
        .unwrap();
        assert_eq!(side, "audio", "{text}");
        assert!(
            text.contains("held 290 ms") && text.contains("audio buffer 280"),
            "{text}"
        );
    }
}
