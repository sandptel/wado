//! A COSE public key (RFC 9053) — the one kind this daemon accepts: EC2 on P-256 for ES256.

use super::cbor::Cbor;

const KTY: i128 = 1;
const ALG: i128 = 3;
const CRV: i128 = -1;
const X: i128 = -2;
const Y: i128 = -3;
const KTY_EC2: i128 = 2;
const ALG_ES256: i128 = -7;
const CRV_P256: i128 = 1;

/// The key as `04 ‖ x ‖ y`, which is how `ring` takes it.
pub fn p256(c: &Cbor) -> Result<[u8; 65], &'static str> {
    if c.at(KTY).and_then(Cbor::int) != Some(KTY_EC2) {
        return Err("the passkey's key is not an elliptic-curve key");
    }
    if c.at(ALG).and_then(Cbor::int) != Some(ALG_ES256) {
        return Err("the passkey does not use ES256");
    }
    if c.at(CRV).and_then(Cbor::int) != Some(CRV_P256) {
        return Err("the passkey's key is not on P-256");
    }
    let (Some(x), Some(y)) = (c.at(X).and_then(Cbor::bytes), c.at(Y).and_then(Cbor::bytes)) else {
        return Err("the passkey's key has no coordinates");
    };
    if x.len() != 32 || y.len() != 32 {
        return Err("the passkey's key coordinates are the wrong size");
    }
    let mut k = [0; 65];
    k[0] = 4;
    k[1..33].copy_from_slice(x);
    k[33..].copy_from_slice(y);
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(alg: i128) -> Cbor {
        Cbor::Map(vec![
            (Cbor::Int(KTY), Cbor::Int(2)),
            (Cbor::Int(ALG), Cbor::Int(alg)),
            (Cbor::Int(CRV), Cbor::Int(1)),
            (Cbor::Int(X), Cbor::Bytes(vec![1; 32])),
            (Cbor::Int(Y), Cbor::Bytes(vec![2; 32])),
        ])
    }

    #[test]
    fn es256_only() {
        let k = p256(&key(-7)).unwrap();
        assert_eq!((k[0], k[1], k[64]), (4, 1, 2));
        assert!(p256(&key(-8)).is_err(), "EdDSA");
        assert!(p256(&key(-257)).is_err(), "RS256");
        assert!(p256(&Cbor::Null).is_err());
    }
}
