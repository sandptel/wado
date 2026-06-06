// M2: virtual input injection (phone → compositor).
// PointerHandle / KeyboardHandle / TouchHandle will be driven from here
// once the transport layer delivers input packets from the remote client.
#[allow(dead_code)]
pub struct VirtualInput;

#[allow(dead_code)]
impl VirtualInput {
    pub fn new() -> Self {
        Self
    }
}
