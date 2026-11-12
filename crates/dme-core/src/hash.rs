use md5::Md5;
use sha1::{Digest, Sha1};

pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&Md5::digest(data));
    out
}

pub fn sha1_hex(data: &[u8]) -> String {
    hex_upper(&Sha1::digest(data))
}

pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

pub fn signature_crc_hex(data: &[u8]) -> String {
    hex_upper(&crc32(data).to_le_bytes())
}

pub fn hex_upper(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(sha1_hex(b"abc"), "A9993E364706816ABA3E25717850C26C9CD0D89D");
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(hex_upper(&md5(b"")), "D41D8CD98F00B204E9800998ECF8427E");
    }

    #[test]
    fn signature_crc_is_little_endian() {
        assert_eq!(signature_crc_hex(b"123456789"), "2639F4CB");
    }
}
