//! VNC authentication: DES-encrypt the server's 16-byte challenge with the password,
//! whose bytes are bit-reversed as RFB specifies.
use des::{
    cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit},
    Des,
};

pub fn response(password: &str, challenge: &[u8; 16]) -> [u8; 16] {
    let mut key = [0u8; 8];
    for (slot, byte) in key.iter_mut().zip(password.bytes()) {
        *slot = byte.reverse_bits();
    }
    let cipher = Des::new(GenericArray::from_slice(&key));
    let mut out = *challenge;
    for block in out.as_chunks_mut::<8>().0 {
        cipher.encrypt_block(GenericArray::from_mut_slice(block));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_an_independent_des_computation() {
        // openssl enc -des-ecb -K cea6c64ea62e0000 -nopad of "0123456789abcdef" ("secret" bit-reversed).
        let expected = [0x75, 0x24, 0x40, 0xee, 0x2b, 0xfc, 0xc2, 0xa0, 0xd9, 0x01, 0x3f, 0xd2, 0x03, 0x71, 0xe2, 0x3b];
        assert_eq!(super::response("secret", b"0123456789abcdef"), expected);
        // Only the first eight password bytes count.
        assert_eq!(super::response("secretXYZ!", b"0123456789abcdef")[..8], super::response("secretXY", b"0123456789abcdef")[..8]);
    }
}
