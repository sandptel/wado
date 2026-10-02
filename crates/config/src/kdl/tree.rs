//! The KDL document as a JSON value, with a position for every key.
//!
//! The mapping, which is the whole config grammar:
//!
//! | KDL                        | value                              |
//! |----------------------------|------------------------------------|
//! | `flag`                     | `true`                             |
//! | `key 1`                    | `1`                                |
//! | `key "a" "b"`              | `["a", "b"]`                       |
//! | `key a=1 b=2`              | `{"a": 1, "b": 2}`                 |
//! | `key { … }`                | `{…}`                              |
//! | repeated `key`             | later wins (objects merge per key) |
//! | repeated list node         | appended (`schema::LIST_NODES`)    |
//! | `include "f.kdl"`          | `f.kdl` read in place, relative    |
//! | `include "f.kdl" optional=#true` | …and skipped if missing      |

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use kdl::{KdlDocument, KdlNode, KdlValue};
use serde_json::{Map, Value};

use crate::{ConfigError, schema::LIST_NODES};

/// An include chain deeper than this is a cycle in practice.
const MAX_INCLUDE_DEPTH: usize = 8;

/// Where each key path (`stream.max-fps`, `session.autostart[1]`) was written.
#[derive(Default)]
pub struct Spans {
    at: HashMap<String, (usize, usize)>,
    files: Vec<(PathBuf, String)>,
}

impl Spans {
    fn file(&mut self, path: &Path, src: &str) -> usize {
        self.files.push((path.to_path_buf(), src.to_string()));
        self.files.len() - 1
    }

    /// An error at the longest recorded prefix of `path` — an unknown field reports the path of
    /// the struct it was found in, so the prefix is as close as serde lets us get.
    pub fn error(&self, path: &str, message: String) -> ConfigError {
        let mut p = path;
        loop {
            if let Some(&(file, offset)) = self.at.get(p) {
                let (f, src) = &self.files[file];
                return ConfigError::at(f.clone(), src, offset, message);
            }
            match p.rfind(['.', '[']) {
                Some(i) => p = &p[..i],
                None => break,
            }
        }
        let f = self
            .files
            .first()
            .map(|(f, _)| f.clone())
            .unwrap_or_default();
        ConfigError {
            file: f,
            line: 0,
            col: 0,
            message,
        }
    }
}

pub fn read(path: &Path, spans: &mut Spans) -> Result<Value, ConfigError> {
    let src = std::fs::read_to_string(path).map_err(|e| ConfigError::io(path.into(), &e))?;
    read_str(path, &src, spans)
}

pub fn read_str(path: &Path, src: &str, spans: &mut Spans) -> Result<Value, ConfigError> {
    let mut root = Map::new();
    collect(path, src, &mut root, "", spans, 0)?;
    Ok(Value::Object(root))
}

fn collect(
    path: &Path,
    src: &str,
    out: &mut Map<String, Value>,
    prefix: &str,
    spans: &mut Spans,
    depth: usize,
) -> Result<(), ConfigError> {
    let file = spans.file(path, src);
    let doc: KdlDocument = src.parse().map_err(|e: kdl::KdlError| {
        let d = e.diagnostics.first();
        let offset = d.map_or(0, |d| d.span.offset());
        let msg = d
            .map(|d| match (&d.message, &d.help) {
                (Some(m), Some(h)) => format!("{m} ({h})"),
                (Some(m), None) => m.clone(),
                _ => "invalid KDL".into(),
            })
            .unwrap_or_else(|| "invalid KDL".into());
        ConfigError::at(path.into(), src, offset, msg)
    })?;
    nodes(path, src, file, doc.nodes(), out, prefix, spans, depth)
}

#[allow(clippy::too_many_arguments)]
fn nodes(
    path: &Path,
    src: &str,
    file: usize,
    list: &[KdlNode],
    out: &mut Map<String, Value>,
    prefix: &str,
    spans: &mut Spans,
    depth: usize,
) -> Result<(), ConfigError> {
    for node in list {
        let name = node.name().value();
        let err = |m: String| ConfigError::at(path.into(), src, node.span().offset(), m);

        if name == "include" {
            include(path, node, out, prefix, spans, depth).map_err(|e| {
                if e.line == 0 && e.file == path {
                    err(e.message)
                } else {
                    e
                }
            })?;
            continue;
        }

        let key = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        let value = node_value(path, src, file, node, &key, spans, depth)?;

        if LIST_NODES.contains(&name) {
            let slot = out.entry(name).or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(items) = slot else {
                return Err(err(format!("`{name}` cannot be both a list and a value")));
            };
            spans.at.insert(
                format!("{key}[{}]", items.len()),
                (file, node.span().offset()),
            );
            spans.at.entry(key).or_insert((file, node.span().offset()));
            items.push(value);
        } else {
            spans.at.insert(key, (file, node.span().offset()));
            match out.get_mut(name) {
                Some(slot) => super::merge::deep(slot, value),
                None => {
                    out.insert(name.to_string(), value);
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn node_value(
    path: &Path,
    src: &str,
    file: usize,
    node: &KdlNode,
    key: &str,
    spans: &mut Spans,
    depth: usize,
) -> Result<Value, ConfigError> {
    let args: Vec<Value> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| scalar(e.value()))
        .collect();
    let props: Vec<_> = node
        .entries()
        .iter()
        .filter_map(|e| Some((e.name()?.value(), e)))
        .collect();

    if props.is_empty() && node.children().is_none() {
        return Ok(match args.len() {
            0 => Value::Bool(true),
            1 => args.into_iter().next().unwrap_or(Value::Null),
            _ => Value::Array(args),
        });
    }
    if !args.is_empty() {
        return Err(ConfigError::at(
            path.into(),
            src,
            node.span().offset(),
            format!(
                "`{}` takes either values or a block/properties, not both",
                node.name().value()
            ),
        ));
    }

    let mut obj = Map::new();
    for (k, e) in props {
        spans
            .at
            .insert(format!("{key}.{k}"), (file, e.span().offset()));
        obj.insert(k.to_string(), scalar(e.value()));
    }
    if let Some(children) = node.children() {
        nodes(
            path,
            src,
            file,
            children.nodes(),
            &mut obj,
            key,
            spans,
            depth,
        )?;
    }
    Ok(Value::Object(obj))
}

fn include(
    from: &Path,
    node: &KdlNode,
    out: &mut Map<String, Value>,
    prefix: &str,
    spans: &mut Spans,
    depth: usize,
) -> Result<(), ConfigError> {
    let bad = |m: &str| ConfigError {
        file: from.into(),
        line: 0,
        col: 0,
        message: m.into(),
    };
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(bad("includes nested too deep (a cycle?)"));
    }
    let Some(KdlValue::String(rel)) = node.get(0) else {
        return Err(bad("`include` needs a file name"));
    };
    let optional = matches!(node.get("optional"), Some(KdlValue::Bool(true)));
    let target = from.parent().unwrap_or(Path::new(".")).join(rel);
    match std::fs::read_to_string(&target) {
        Ok(src) => collect(&target, &src, out, prefix, spans, depth + 1),
        Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ConfigError::io(target, &e)),
    }
}

fn scalar(v: &KdlValue) -> Value {
    match v {
        KdlValue::String(s) => Value::String(s.clone()),
        KdlValue::Integer(i) => i64::try_from(*i).map(Value::from).unwrap_or(Value::Null),
        KdlValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        KdlValue::Bool(b) => Value::Bool(*b),
        KdlValue::Null => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_reads_in_place_and_later_wins() {
        let dir = std::env::temp_dir().join(format!("wado-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ui.kdl"),
            "stream { max-fps 60 }\nsession { autostart \"b\" }",
        )
        .unwrap();
        let main = dir.join("config.kdl");
        std::fs::write(
            &main,
            "stream { max-fps 120; max-bitrate 9000 }\nsession { autostart \"a\" }\ninclude \"ui.kdl\"\ninclude \"none.kdl\" optional=#true",
        )
        .unwrap();
        let c = crate::kdl::load(&main).unwrap();
        assert_eq!(c.stream.max_fps, Some(60));
        assert_eq!(c.stream.max_bitrate, Some(9000));
        assert_eq!(c.session.autostart, ["a", "b"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_missing_required_include_is_an_error() {
        let e = crate::kdl::parse("include \"/nonexistent/x.kdl\"").unwrap_err();
        assert!(e.file.ends_with("x.kdl"), "{e}");
    }
}
