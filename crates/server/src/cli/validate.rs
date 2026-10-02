//! `wado validate [file]` — check a config without starting anything.

use std::path::PathBuf;

pub fn run(file: Option<&str>) -> i32 {
    let path = file
        .map(PathBuf::from)
        .unwrap_or_else(wado_config::paths::config_file);
    match wado_config::load(&path) {
        Ok((_, env)) => {
            println!("{}: ok", path.display());
            if !env.is_empty() {
                println!("overridden by environment: {}", env.join(", "));
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
