//! `wado validate [file]` — check a config without starting anything.

use std::path::PathBuf;

pub fn run(file: Option<&str>) -> i32 {
    let path = file
        .map(PathBuf::from)
        .unwrap_or_else(wado_config::paths::config_file);
    match wado_config::load(&path) {
        Ok(l) => {
            let files: Vec<_> = l.files.iter().map(|f| f.display().to_string()).collect();
            println!("ok: {}", files.join(", "));
            if !l.env.is_empty() {
                println!("overridden by environment: {}", l.env.join(", "));
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
