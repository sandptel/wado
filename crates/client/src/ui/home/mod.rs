//! Home: a lobby of computers, plus the few things wanted without opening one. The screen itself
//! is framed by [`crate::ui::landing`]; these are its parts.
//!
//! | file | job |
//! |---|---|
//! | [`cards`] | the computer cards, each with ▶ straight to its desktop |
//! | [`sessions`] | Running: the sessions on the selected computer |
//! | [`settings`] | On this phone: the app-wide settings list |
//! | [`widgets`] | the widgets, each bound to one computer, and arranging them |
//! | [`pages`] | the phone's own multi-section pages |

pub mod cards;
pub mod pages;
pub mod sessions;
pub mod settings;
pub mod widgets;
