//! A capability as the registry holds it: provided by someone, or absent for a stated reason.
//!
//! Absence is never silent. An `Absent` says why in words a person can act on, and which
//! providers were tried, so the client can hide the button *and* say what to install.

use std::sync::Arc;
use std::time::Instant;

/// Who provides a capability: `"linux"`, `"hyprland"`, `"fake"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProviderId(pub &'static str);

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Why a capability is missing.
#[derive(Debug, Clone, PartialEq)]
pub struct Absent {
    /// The reason, in words a person can act on — the last provider's, or "nothing here
    /// provides it".
    pub why: String,
    /// Every provider that offered it and failed to open, with its reason, in the order tried.
    pub tried: Vec<(ProviderId, String)>,
    /// When it was found missing — a stale absence is worth probing again.
    pub at: Instant,
}

impl Absent {
    /// Absent for `why`, with nothing tried.
    pub fn because(why: impl Into<String>) -> Absent {
        Absent {
            why: why.into(),
            tried: Vec::new(),
            at: Instant::now(),
        }
    }
}

impl std::fmt::Display for Absent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

/// One capability: on, with its implementation and who provides it — or off, and why.
pub enum Cap<T: ?Sized> {
    On { imp: Arc<T>, by: ProviderId },
    Off(Absent),
}

impl<T: ?Sized> Cap<T> {
    /// The implementation, or why there is none.
    pub fn get(&self) -> Result<Arc<T>, Absent> {
        match self {
            Cap::On { imp, .. } => Ok(Arc::clone(imp)),
            Cap::Off(a) => Err(a.clone()),
        }
    }

    pub fn is_on(&self) -> bool {
        matches!(self, Cap::On { .. })
    }

    /// Who provides it, when someone does.
    pub fn by(&self) -> Option<ProviderId> {
        match self {
            Cap::On { by, .. } => Some(*by),
            Cap::Off(_) => None,
        }
    }
}

impl<T: ?Sized> Clone for Cap<T> {
    fn clone(&self) -> Self {
        match self {
            Cap::On { imp, by } => Cap::On {
                imp: Arc::clone(imp),
                by: *by,
            },
            Cap::Off(a) => Cap::Off(a.clone()),
        }
    }
}
