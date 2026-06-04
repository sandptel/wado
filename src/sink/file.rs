use std::{fs::File, io::Write, path::Path};

use super::FrameSink;

#[allow(dead_code)]
pub struct FileSink {
    file: File,
}

#[allow(dead_code)]
impl FileSink {
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self { file: File::create(path)? })
    }
}

impl FrameSink for FileSink {
    fn send(&mut self, nal_data: &[u8]) {
        let _ = self.file.write_all(nal_data);
    }
}
