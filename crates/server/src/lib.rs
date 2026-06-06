#![allow(irrefutable_let_patterns)]

pub mod capture;
pub mod conf;
pub mod encode;
pub mod error;
pub mod grabs;
pub mod handlers;
pub mod headless;
pub mod input;
pub mod sink;
pub mod state;
pub mod website;

pub use error::{Result, WadoError};
pub use state::Wado;
