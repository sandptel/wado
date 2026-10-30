//! Just enough CBOR (RFC 8949) to read a WebAuthn attestation object and the COSE key inside it.
//!
//! Hand-written rather than a crate: the only CBOR this daemon ever reads is those two small
//! structures, from a device that already passed the envelope. Definite lengths only (WebAuthn
//! requires CTAP2 canonical CBOR, which has no indefinite ones), no tags, no floats, and a depth
//! cap, so a hostile input can neither recurse nor claim more bytes than it sent.

#[derive(Debug, Clone, PartialEq)]
pub enum Cbor {
    /// Major types 0 and 1, as one signed value.
    Int(i128),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
    Bool(bool),
    Null,
}

impl Cbor {
    /// The value under integer key `k` of a map.
    pub fn at(&self, k: i128) -> Option<&Cbor> {
        self.find(|key| *key == Cbor::Int(k))
    }

    /// The value under text key `k` of a map.
    pub fn get(&self, k: &str) -> Option<&Cbor> {
        self.find(|key| matches!(key, Cbor::Text(t) if t == k))
    }

    fn find(&self, pick: impl Fn(&Cbor) -> bool) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m.iter().find(|(key, _)| pick(key)).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i128> {
        match self {
            Cbor::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Cbor::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 8;

/// One item from the start of `b`, and how many bytes it took.
pub fn decode(b: &[u8]) -> Result<(Cbor, usize), &'static str> {
    let mut at = 0;
    let v = item(b, &mut at, 0)?;
    Ok((v, at))
}

fn take<'a>(b: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], &'static str> {
    let end = at.checked_add(n).ok_or("cbor: length overflow")?;
    let s = b.get(*at..end).ok_or("cbor: truncated")?;
    *at = end;
    Ok(s)
}

fn argument(b: &[u8], at: &mut usize, info: u8) -> Result<u64, &'static str> {
    Ok(match info {
        0..=23 => u64::from(info),
        24 => u64::from(take(b, at, 1)?[0]),
        25 => u64::from(u16::from_be_bytes(
            take(b, at, 2)?.try_into().unwrap_or_default(),
        )),
        26 => u64::from(u32::from_be_bytes(
            take(b, at, 4)?.try_into().unwrap_or_default(),
        )),
        27 => u64::from_be_bytes(take(b, at, 8)?.try_into().unwrap_or_default()),
        _ => return Err("cbor: indefinite or reserved length"),
    })
}

fn item(b: &[u8], at: &mut usize, depth: usize) -> Result<Cbor, &'static str> {
    if depth > MAX_DEPTH {
        return Err("cbor: nested too deep");
    }
    let first = take(b, at, 1)?[0];
    let (major, info) = (first >> 5, first & 0x1f);
    if major == 7 {
        return match info {
            20 => Ok(Cbor::Bool(false)),
            21 => Ok(Cbor::Bool(true)),
            22 => Ok(Cbor::Null),
            _ => Err("cbor: unsupported simple value"),
        };
    }
    let n = argument(b, at, info)?;
    // Every element takes at least one byte, so a count beyond what is left is a lie.
    let left = (b.len() - *at) as u64;
    Ok(match major {
        0 => Cbor::Int(i128::from(n)),
        1 => Cbor::Int(-1 - i128::from(n)),
        2 => Cbor::Bytes(take(b, at, usize::try_from(n).map_err(|_| "cbor: too long")?)?.to_vec()),
        3 => {
            let s = take(b, at, usize::try_from(n).map_err(|_| "cbor: too long")?)?;
            Cbor::Text(String::from_utf8(s.to_vec()).map_err(|_| "cbor: text is not UTF-8")?)
        }
        4 => {
            if n > left {
                return Err("cbor: truncated");
            }
            let mut v = Vec::with_capacity(n as usize);
            for _ in 0..n {
                v.push(item(b, at, depth + 1)?);
            }
            Cbor::Array(v)
        }
        5 => {
            if n.saturating_mul(2) > left {
                return Err("cbor: truncated");
            }
            let mut m = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let k = item(b, at, depth + 1)?;
                let v = item(b, at, depth + 1)?;
                m.push((k, v));
            }
            Cbor::Map(m)
        }
        _ => return Err("cbor: tags are not supported"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_rfc_examples_it_needs() {
        // RFC 8949 appendix A.
        assert_eq!(decode(&[0x17]).unwrap(), (Cbor::Int(23), 1));
        assert_eq!(decode(&[0x18, 0x64]).unwrap().0, Cbor::Int(100));
        assert_eq!(decode(&[0x39, 0x03, 0xe7]).unwrap().0, Cbor::Int(-1000));
        assert_eq!(decode(&[0x20]).unwrap().0, Cbor::Int(-1));
        assert_eq!(
            decode(&[0x44, 1, 2, 3, 4]).unwrap().0,
            Cbor::Bytes(vec![1, 2, 3, 4])
        );
        assert_eq!(
            decode(&[0x62, 0x22, 0x5c]).unwrap().0,
            Cbor::Text("\"\\".into())
        );
        let m = decode(&[0xa2, 0x01, 0x02, 0x03, 0x04]).unwrap().0;
        assert_eq!(m.at(3), Some(&Cbor::Int(4)));
        let t = decode(&[0xa1, 0x61, 0x61, 0x01]).unwrap().0;
        assert_eq!(t.get("a"), Some(&Cbor::Int(1)));
        assert_eq!(decode(&[0xf5]).unwrap().0, Cbor::Bool(true));
    }

    #[test]
    fn hostile_input_is_refused_not_trusted() {
        assert!(decode(&[]).is_err());
        assert!(
            decode(&[0x5a, 0xff, 0xff, 0xff, 0xff]).is_err(),
            "claims 4 GB"
        );
        assert!(decode(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).is_err());
        assert!(decode(&[0x5f]).is_err(), "indefinite length");
        assert!(decode(&[0xc0, 0x00]).is_err(), "a tag");
        assert!(decode(&[0x81; 64]).is_err(), "too deep");
        assert!(decode(&[0x62, 0xff, 0xfe]).is_err(), "not UTF-8");
    }
}
