// SPDX-License-Identifier: AGPL-3.0-only
//! Compiles the `.slint` UI into Rust at build time: no interpreter, no runtime parsing.

fn main() {
    slint_build::compile("ui/app.slint").expect("the UI compiles");
}
