//! A typed pairing's check words: four bytes of the handshake's short authentication string as
//! four words from the list both ends share (`wado_protocol::envelope::SAS_WORDS`).

use wado_protocol::envelope;

/// The four words for `sas` (its first four bytes; fewer reads as `?`).
#[uniffi::export]
pub fn sas_words(sas: Vec<u8>) -> Vec<String> {
    let mut b = [0u8; 4];
    for (i, v) in sas.iter().take(4).enumerate() {
        b[i] = *v;
    }
    let mut out: Vec<String> = envelope::sas_words(b).iter().map(|w| w.to_string()).collect();
    for w in out.iter_mut().skip(sas.len()) {
        *w = "?".into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_the_protocols() {
        let w = sas_words(vec![0, 1, 2, 255]);
        let want: Vec<String> = envelope::sas_words([0, 1, 2, 255])
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(w, want);
        assert_eq!(sas_words(vec![7])[1..], ["?", "?", "?"]);
    }
}
