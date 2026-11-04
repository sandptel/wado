//! The clients' shared experience model (`2026-11-04`): what both the web client and the app
//! decide the same way, written once. Pure functions over serde types, no I/O — so the web calls
//! them directly and the app through one stable bridge call ([`op::call`], app-bridge `ux`),
//! and a feature built on one side ports to the other by calling the same function. A client
//! that wants something of its own still draws and wires it itself; this holds only the rules.
//!
//! | module | job |
//! |---|---|
//! | [`form`] | phone, tablet or desktop, from the screen and the pointer |
//! | [`game`] | Game mode: a profile, its presets, the settings it shadows |
//! | [`poster`] | the accent slot a poster washes in |
//! | [`help`] | what each gesture or control does here, for the `?` overlay |
//! | [`recent`] | Continue and "played lately" |
//! | [`trust`] | how long a device has, its grants in words |
//! | [`qr`] | a QR as an SVG path |
//! | [`tour`] | the first-run cards |
//! | [`op`] | the JSON door the app's bridge calls |

pub mod form;
pub mod game;
pub mod help;
pub mod op;
pub mod poster;
pub mod qr;
pub mod recent;
pub mod tour;
pub mod trust;
