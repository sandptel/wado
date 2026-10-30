//! WebAuthn authenticator data: which site it signed for, what the person did, its counter,
//! and — at registration — the new credential and its public key.
//!
//! ```text
//!   rpIdHash 32 │ flags 1 │ signCount 4 │ [ AAGUID 16 │ credIdLen 2 │ credId │ COSE key ] │ …
//! ```

use super::cbor;

pub const UP: u8 = 0x01;
pub const UV: u8 = 0x04;
pub const AT: u8 = 0x40;

#[derive(Debug, Clone, PartialEq)]
pub struct AuthData {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub count: u32,
    /// The credential made, present when `flags` has [`AT`].
    pub attested: Option<Attested>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attested {
    pub cred_id: Vec<u8>,
    /// The public key, uncompressed: `04 ‖ x ‖ y`.
    pub key: [u8; 65],
}

pub fn parse(b: &[u8]) -> Result<AuthData, &'static str> {
    if b.len() < 37 {
        return Err("authenticator data is too short");
    }
    let mut rp_id_hash = [0; 32];
    rp_id_hash.copy_from_slice(&b[..32]);
    let flags = b[32];
    let count = u32::from_be_bytes([b[33], b[34], b[35], b[36]]);
    let attested = if flags & AT != 0 {
        let rest = &b[37..];
        if rest.len() < 18 {
            return Err("authenticator data has no room for the credential");
        }
        let n = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
        let cred_id = rest
            .get(18..18 + n)
            .ok_or("authenticator data: credential id is truncated")?
            .to_vec();
        if cred_id.is_empty() || n > 1023 {
            return Err("authenticator data: credential id length is out of range");
        }
        let (cose, _) = cbor::decode(&rest[18 + n..])?;
        Some(Attested {
            cred_id,
            key: super::cose::p256(&cose)?,
        })
    } else {
        None
    };
    Ok(AuthData {
        rp_id_hash,
        flags,
        count,
        attested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_or_truncated_data_is_refused() {
        assert!(parse(&[0; 36]).is_err());
        let mut b = vec![0; 37];
        assert_eq!(parse(&b).unwrap().attested, None);
        b[32] = AT;
        assert!(parse(&b).is_err(), "AT set, nothing after");
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&[0, 40, 1, 2]); // claims 40 bytes of id, has 2
        assert!(parse(&b).is_err());
    }

    #[test]
    fn reads_flags_and_counter() {
        let mut b = vec![9; 32];
        b.push(UP | UV);
        b.extend_from_slice(&7u32.to_be_bytes());
        let a = parse(&b).unwrap();
        assert_eq!((a.flags, a.count, a.rp_id_hash[0]), (UP | UV, 7, 9));
    }
}
