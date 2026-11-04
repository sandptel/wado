//! The one JSON door: `call(op, json) -> json`. The app's bridge exposes exactly this
//! (app-bridge `ux`), so a new shared rule needs no new binding — add an arm here and call it by
//! name from Dart. The web calls the modules directly. Unknown ops and bad input answer
//! `{"error": …}`, never panic.

use serde_json::{json, Value};

use crate::{form::Form, game, help, poster, qr, recent, tour, trust};

fn arg<T: serde::de::DeserializeOwned>(v: &Value, k: &str) -> Option<T> {
    serde_json::from_value(v.get(k)?.clone()).ok()
}

pub fn call(op: &str, input: &str) -> String {
    let v: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    let out = match op {
        "form" => json!(Form::of(
            v["w"].as_f64().unwrap_or(0.0),
            v["h"].as_f64().unwrap_or(0.0),
            v["touch"].as_bool().unwrap_or(true)
        )),
        "hue" => json!(poster::hue(&s("name"))),
        "new_game" => json!(game::new(&s("name"), &s("exec"))),
        "overlay" => match arg::<game::Profile>(&v, "profile") {
            Some(p) => json!(p.overlay(n("refresh") as u32)),
            None => json!({ "error": "no profile" }),
        },
        "keep" => match arg::<game::Profile>(&v, "profile") {
            Some(mut p) => {
                p.keep(&s("key"), v.get("value").cloned().unwrap_or(Value::Null));
                json!(p)
            }
            None => json!({ "error": "no profile" }),
        },
        "picture_words" => json!(game::picture_words(&s("id"), n("refresh") as u32)),
        "needs_x" => json!(game::needs_x(&s("exec"))),
        "help" => match serde_json::from_value::<help::Input>(v.clone()) {
            Ok(i) => json!(help::rows(&i)),
            Err(e) => json!({ "error": e.to_string() }),
        },
        "note" => {
            let list: Vec<recent::Recent> = arg(&v, "list").unwrap_or_default();
            match arg::<recent::Recent>(&v, "recent") {
                Some(r) => json!(recent::note(&list, r)),
                None => json!(list),
            }
        }
        "gamed_lately" => json!(recent::gamed_lately(
            &arg::<Vec<recent::Recent>>(&v, "list").unwrap_or_default(),
            &s("computer"),
            n("now")
        )),
        "ago" => json!(recent::ago(n("at"), n("now"))),
        "left" => json!(trust::left(
            v.get("until").and_then(Value::as_u64),
            n("now")
        )),
        "grants" => json!(trust::grants(&s("tokens"))),
        "qr" => match qr::path(&s("text")) {
            Some((size, d)) => json!({ "size": size, "d": d }),
            None => json!({ "error": "too long for a QR code" }),
        },
        "tour" => json!(tour::cards(
            &s("computer"),
            arg(&v, "form").unwrap_or_default()
        )),
        _ => json!({ "error": format!("no such op: {op}") }),
    };
    out.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_door_answers_and_never_panics() {
        assert_eq!(super::call("needs_x", r#"{"exec":"steam"}"#), "true");
        assert!(super::call("nope", "{}").contains("no such op"));
        assert!(super::call("overlay", "garbage").contains("error"));
        let o = super::call(
            "overlay",
            r#"{"profile":{"name":"A","exec":"a"},"refresh":90}"#,
        );
        assert!(o.contains(r#""fps":90"#));
    }
}
