//! The computer's sound: every output and everything playing, read from `pw-dump`, changed with
//! `wpctl` (volume, mute, default) and `pw-metadata` (moving a stream — WirePlumber follows a
//! stream's `target.object`).

use serde_json::Value;
use wado_protocol::host::{HostAudio, Sink, Stream};

use super::cmd::run;

pub async fn state(phone: Option<&str>) -> HostAudio {
    let Ok(json) = run("pw-dump", &[]).await else {
        return HostAudio::default();
    };
    let mut a = parse(&json);
    a.phone_sink = phone.map(str::to_string);
    // Another daemon's "This phone" (a pool has several) is someone else's device: not an
    // output this viewer should send sound to.
    a.sinks
        .retain(|s| !s.name.starts_with("wado-") || Some(s.name.as_str()) == phone);
    if let Some(p) = phone {
        for s in &mut a.sinks {
            if s.name == p {
                s.label = "This phone".into();
            }
        }
    }
    a
}

/// `pw-dump`'s JSON → outputs, streams, the default, and which output each stream is on.
pub fn parse(json: &str) -> HostAudio {
    let objs: Vec<Value> = serde_json::from_str(json).unwrap_or_default();
    let nodes = || {
        objs.iter()
            .filter(|o| o["type"] == "PipeWire:Interface:Node")
    };
    let name_of = |id: u64| {
        nodes()
            .find(|o| o["id"].as_u64() == Some(id))
            .and_then(|o| o["info"]["props"]["node.name"].as_str())
            .map(str::to_string)
    };
    // Volume is stored cubed (`channelVolumes`); the cube root is the slider `wpctl` shows.
    let vol = |o: &Value| -> (f32, bool) {
        let props = o["info"]["params"]["Props"].as_array();
        let p = props.and_then(|ps| ps.iter().find(|p| p.get("channelVolumes").is_some()));
        let v = p
            .and_then(|p| p["channelVolumes"].as_array())
            .and_then(|vs| vs.iter().filter_map(Value::as_f64).reduce(f64::max))
            .unwrap_or(1.0);
        let muted = p.and_then(|p| p["mute"].as_bool()).unwrap_or(false);
        ((v.cbrt() * 100.0).round() as f32 / 100.0, muted)
    };
    let prop = |o: &Value, k: &str| o["info"]["props"][k].as_str().unwrap_or("").to_string();

    let sinks = nodes()
        .filter(|o| o["info"]["props"]["media.class"] == "Audio/Sink")
        .map(|o| {
            let (volume, muted) = vol(o);
            let name = prop(o, "node.name");
            let desc = prop(o, "node.description");
            Sink {
                id: o["id"].as_u64().unwrap_or(0) as u32,
                label: if desc.is_empty() { name.clone() } else { desc },
                name,
                volume,
                muted,
            }
        })
        .collect();
    let streams = nodes()
        .filter(|o| o["info"]["props"]["media.class"] == "Stream/Output/Audio")
        .map(|o| {
            let id = o["id"].as_u64().unwrap_or(0);
            let (volume, muted) = vol(o);
            // Where it plays now: the node its output is linked to.
            let sink = objs
                .iter()
                .filter(|l| {
                    l["type"] == "PipeWire:Interface:Link"
                        && l["info"]["output-node-id"].as_u64() == Some(id)
                })
                .find_map(|l| l["info"]["input-node-id"].as_u64().and_then(name_of));
            let app = [prop(o, "application.name"), prop(o, "node.name")]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            let pid = o["info"]["props"]["application.process.id"]
                .as_u64()
                .or_else(|| prop(o, "application.process.id").parse().ok())
                .map(|p| p as u32);
            Stream {
                id: id as u32,
                app,
                title: prop(o, "media.name"),
                volume,
                muted,
                sink,
                pid,
            }
        })
        .collect();
    let default_sink = objs
        .iter()
        .filter(|o| {
            o["type"] == "PipeWire:Interface:Metadata" && o["props"]["metadata.name"] == "default"
        })
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|m| m["key"] == "default.audio.sink")
        .and_then(|m| m["value"]["name"].as_str().map(str::to_string))
        .unwrap_or_default();
    HostAudio {
        sinks,
        streams,
        default_sink,
        phone_sink: None,
    }
}

pub async fn set_volume(id: u32, v: f32) -> Result<(), String> {
    run(
        "wpctl",
        &[
            "set-volume",
            &id.to_string(),
            &format!("{:.2}", v.clamp(0.0, 1.5)),
        ],
    )
    .await
    .map(drop)
}

pub async fn set_mute(id: u32, muted: bool) -> Result<(), String> {
    run(
        "wpctl",
        &["set-mute", &id.to_string(), if muted { "1" } else { "0" }],
    )
    .await
    .map(drop)
}

pub async fn set_default(name: &str, sinks: &[Sink]) -> Result<(), String> {
    let s = sinks
        .iter()
        .find(|s| s.name == name)
        .ok_or("no such output")?;
    run("wpctl", &["set-default", &s.id.to_string()])
        .await
        .map(drop)
}

pub async fn move_stream(id: u32, sink: &str) -> Result<(), String> {
    run("pw-metadata", &[&id.to_string(), "target.object", sink])
        .await
        .map(drop)
}

/// Everything to one output: the default (so what starts next goes there) and every stream now.
pub async fn all_to(sink: &str, a: &HostAudio) -> Result<(), String> {
    set_default(sink, &a.sinks).await?;
    for s in &a.streams {
        move_stream(s.id, sink).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_sinks_streams_links_and_the_default() {
        let json = r#"[
          {"id":53,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"spk","node.description":"Speaker"},
            "params":{"Props":[{"channelVolumes":[0.000216,0.000216],"mute":false}]}}},
          {"id":97,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"wado-1","node.description":"x"},"params":{}}},
          {"id":98,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","application.name":"Firefox","media.name":"Video"},
            "params":{"Props":[{"channelVolumes":[1.0],"mute":true}]}}},
          {"id":200,"type":"PipeWire:Interface:Link","info":{"output-node-id":98,"input-node-id":53}},
          {"id":41,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
            "metadata":[{"subject":0,"key":"default.audio.sink","value":{"name":"spk"}}]}
        ]"#;
        let a = super::parse(json);
        assert_eq!(a.sinks.len(), 2);
        assert_eq!(a.sinks[0].volume, 0.06, "cube root of the stored volume");
        assert_eq!(a.streams[0].app, "Firefox");
        assert!(a.streams[0].muted);
        assert_eq!(a.streams[0].sink.as_deref(), Some("spk"));
        assert_eq!(a.default_sink, "spk");
    }
}
