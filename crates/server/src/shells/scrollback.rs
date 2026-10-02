//! A shell's recent output, kept so a reattaching viewer sees what it missed.

/// Bytes of output kept per shell.
pub const CAP: usize = 256 * 1024;

#[derive(Default)]
pub struct Scrollback(String);

impl Scrollback {
    pub fn push(&mut self, data: &str) {
        self.0.push_str(data);
        if self.0.len() > CAP {
            // Cut at a line start where there is one, so the replay does not begin mid-escape
            // sequence more often than it must; always at a char boundary.
            let mut cut = self.0.len() - CAP;
            while !self.0.is_char_boundary(cut) {
                cut += 1;
            }
            if let Some(nl) = self.0[cut..].find('\n') {
                cut += nl + 1;
            }
            self.0.drain(..cut);
        }
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_tail_within_the_cap() {
        let mut s = Scrollback::default();
        let line = "é".repeat(100) + "\n";
        for _ in 0..3000 {
            s.push(&line);
        }
        assert!(s.text().len() <= CAP);
        assert!(s.text().starts_with('é'), "cut at a line start");
    }
}
