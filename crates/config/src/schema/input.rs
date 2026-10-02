//! `input { }` — keyboard and pointer behaviour the host pins for every device. Unset means
//! the device's own choice stands; set means it wins over whatever the client sends.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Input {
    /// Keys per second.
    pub repeat_rate: Option<i32>,
    /// Milliseconds before repeat starts.
    pub repeat_delay: Option<i32>,
    pub focus_follows_pointer: Option<bool>,
}
