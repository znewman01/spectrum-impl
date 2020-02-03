//! Spectrum implementation.

use bytes::Bytes;

/// xor bytes in place, a = a ^ b
// TODO: (Performance) xor inplace rather than copying
pub fn xor_bytes(a: &Bytes, b: &Bytes) -> Bytes {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(&a, &b)| a ^ b).collect()
}

pub fn xor_all_bytes(parts: Vec<Bytes>) -> Bytes {
    // xor all the parts together
    let mut res = Bytes::from(vec![0; parts[0].len()]);
    for part in parts {
        res = xor_bytes(&res, &part);
    }

    res
}
