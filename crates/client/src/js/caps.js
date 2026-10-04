// wado bridge — what this browser is, for the Rust side: its device name ("Android · Chrome")
// names the sound output that plays here (ui/host.rs `here`).
setTimeout(() => emit({ type: "caps", device: W.deviceName || "" }), 0);
