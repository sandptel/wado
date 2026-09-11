//! Print what `wado::apps::discover()` finds on this machine.
//!
//! The unit tests cover the `.desktop` parser; they cannot cover the *scan*, which depends on
//! `XDG_DATA_DIRS` and on where this particular system installs its applications — the thing
//! most likely to differ between a NixOS dev shell and a target machine. Run this when an app
//! you expected is missing from the launcher.
//!
//!     cargo run -p wado --example apps_probe

fn main() {
    let apps = wado::apps::discover();
    println!("{} launchable applications", apps.len());
    for app in &apps {
        println!("  {:<36} {}", app.name, app.exec);
    }
}
