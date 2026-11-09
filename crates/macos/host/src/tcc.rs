//! The Mac's privacy grants, as the providers ask them. Without Accessibility every posted event
//! is dropped silently, so input is offered only when it is granted; the reasons name the pane
//! that grants it.

pub const NEEDS_ACCESSIBILITY: &str = "Allow wado in System Settings › Privacy & Security › Accessibility, so it can type and move the pointer.";
pub const NEEDS_SCREEN_RECORDING: &str =
    "Allow wado in System Settings › Privacy & Security › Screen & System Audio Recording.";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Events this process posts reach other apps.
pub fn accessibility() -> bool {
    // SAFETY: no arguments; reads this process's grant.
    unsafe { AXIsProcessTrusted() }
}

/// This process may capture the screen.
pub fn screen_recording() -> bool {
    // SAFETY: no arguments; reads this process's grant.
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Ask for Screen Recording once (macOS shows its prompt, or opens the pane).
pub fn ask_screen_recording() -> bool {
    // SAFETY: no arguments.
    unsafe { CGRequestScreenCaptureAccess() }
}

/// At start, ask macOS for what is missing, once: it shows its own prompts and lists Wado in
/// System Settings, unticked, for the person to allow. Nothing is asked when all is granted.
pub fn ask_missing() {
    use objc2_foundation::{NSDictionary, NSNumber, NSString};
    if !accessibility() {
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let yes = NSNumber::new_bool(true);
        let opts = NSDictionary::from_slices(&[&*key], &[&*yes]);
        // SAFETY: an NSDictionary is a CFDictionary (toll-free bridged), alive for the call.
        unsafe {
            AXIsProcessTrustedWithOptions(
                (&*opts as *const NSDictionary<NSString, NSNumber>).cast(),
            )
        };
    }
    if !screen_recording() {
        ask_screen_recording();
    }
}

/// `Ok` when Accessibility is granted, else the reason.
pub fn need_accessibility() -> Result<(), String> {
    accessibility()
        .then_some(())
        .ok_or_else(|| NEEDS_ACCESSIBILITY.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_probe_is_off_with_accessibility_reason() {
        match need_accessibility() {
            Ok(()) => assert!(accessibility()),
            Err(why) => {
                assert!(!accessibility());
                assert!(why.contains("Privacy & Security › Accessibility"), "{why}");
            }
        }
    }
}
