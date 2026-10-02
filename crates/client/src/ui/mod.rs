//! The client's UI, one job per module.
//!
//! | | |
//! |---|---|
//! | [`landing`] | before a session: hosts, connection progress, Start |
//! | [`cc`] | the control centre: hero, quick tiles, settings list, Apply |
//! | [`pages`] | the control centre's drill-in pages, one per subject |
//! | [`dock`] | ⋯ ○ ◁ over the picture |
//! | [`stage`] | the video and everything drawn on it |
//! | [`console`], [`drawer`] | the shell and the app drawer sheets |
//! | [`widgets`] | the controls the pages are built from |
//!
//! [`live`] and [`gamepad`] hold no UI any more: they push browser-side settings to the bridge.

pub mod cc;
pub mod console;
pub mod dock;
pub mod drawer;
pub mod fetch;
pub mod gamepad;
pub mod gesture;
pub mod health;
pub mod host;
pub mod landing;
pub mod live;
pub mod media;
pub mod pages;
pub mod rejoin;
pub mod stage;
pub mod toast;
pub mod widgets;
