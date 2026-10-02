//! `wado msg <reload | get [key] | set <key> [value] | devices>` — talk to a running daemon.
//!
//! `--instance N` picks a daemon of a pool (default: `WADO_INSTANCE`, else 1). They share one
//! config directory, so `set` and `reload` through any one of them reach all of them.

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

pub fn run(args: &[String]) -> i32 {
    let mut instance = std::env::var("WADO_INSTANCE").unwrap_or_else(|_| "1".into());
    let mut words = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--instance" => instance = it.next().cloned().unwrap_or(instance),
            _ => words.push(a.as_str()),
        }
    }
    let path = crate::config::socket::path_for(&instance);
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no wado daemon at {} ({e})", path.display());
            return 1;
        }
    };
    let mut answer = String::new();
    let sent = stream
        .write_all(format!("{}\n", words.join(" ")).as_bytes())
        .and_then(|()| stream.read_to_string(&mut answer));
    if let Err(e) = sent {
        eprintln!("{e}");
        return 1;
    }
    print!("{answer}");
    i32::from(answer.starts_with("error"))
}
