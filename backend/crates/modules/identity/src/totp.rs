//! Time-based one-time passwords (RFC 6238 over RFC 4226): HMAC-SHA-1, 6 digits, 30-second steps.
//!
//! SHA-1 is what every authenticator app implements for `otpauth://` secrets; HMAC-SHA-1 is not
//! affected by the collision attacks on the bare hash. The functions are pure (time is a
//! parameter) so the RFC 6238 test vectors run as they are published.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

/// Seconds per time step.
pub const STEP_SECONDS: u64 = 30;
/// Digits of a code shown to the user.
pub const DIGITS: u32 = 6;
/// Steps accepted either side of the current one, for clock drift.
pub const WINDOW: i64 = 1;
/// Bytes of a new secret (160 bits, the length RFC 4226 recommends).
pub const SECRET_BYTES: usize = 20;

type HmacSha1 = Hmac<Sha1>;

/// The HOTP value of RFC 4226 for `counter`, as a number below `10^digits`.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac =
        <HmacSha1 as KeyInit>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let binary = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    binary % 10u32.pow(digits)
}

/// The time step a Unix time falls in.
pub fn step_at(unix_seconds: u64) -> u64 {
    unix_seconds / STEP_SECONDS
}

/// The code for the step containing `unix_seconds`, zero-padded to `digits`.
pub fn code_at(secret: &[u8], unix_seconds: u64, digits: u32) -> String {
    let value = hotp(secret, step_at(unix_seconds), digits);
    format!("{value:0width$}", width = digits as usize)
}

/// The step `code` is valid for within [`WINDOW`] steps of `unix_seconds`, if any. Every step in
/// the window is checked without stopping at the first match, so the time taken does not say which
/// step matched.
pub fn matching_step(secret: &[u8], code: &str, unix_seconds: u64) -> Option<u64> {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let current = step_at(unix_seconds) as i64;
    let mut found = None;
    for offset in -WINDOW..=WINDOW {
        let step = current + offset;
        if step < 0 {
            continue;
        }
        let expected = format!(
            "{:0width$}",
            hotp(secret, step as u64, DIGITS),
            width = DIGITS as usize
        );
        if constant_time_eq(expected.as_bytes(), code.as_bytes()) {
            found = Some(step as u64);
        }
    }
    found
}

/// Equal-length byte strings compared without an early exit.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// RFC 4648 base32 without padding, the form authenticator apps read.
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            out.push(BASE32[((buffer >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(BASE32[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Decode base32 (case-insensitive, spaces and padding ignored). `None` on any other character.
pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let value = BASE32
            .iter()
            .position(|&b| b as char == c.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            out.push((buffer >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// A new random secret.
pub fn generate_secret() -> Vec<u8> {
    use rand::RngExt;
    let mut secret = vec![0u8; SECRET_BYTES];
    rand::rng().fill(&mut secret[..]);
    secret
}

/// The `otpauth://` URI authenticator apps import (the form a QR code carries).
pub fn otpauth_uri(issuer: &str, account: &str, secret: &[u8]) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
        percent_encode(issuer),
        percent_encode(account),
        base32_encode(secret),
        percent_encode(issuer),
    )
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 appendix B: the SHA-1 secret is the ASCII string below, and the published codes are
    /// eight digits.
    const RFC_SECRET: &[u8] = b"12345678901234567890";
    const VECTORS: [(u64, &str); 6] = [
        (59, "94287082"),
        (1111111109, "07081804"),
        (1111111111, "14050471"),
        (1234567890, "89005924"),
        (2000000000, "69279037"),
        (20000000000, "65353130"),
    ];

    #[test]
    fn rfc_6238_vectors_at_eight_digits() {
        for (time, expected) in VECTORS {
            assert_eq!(code_at(RFC_SECRET, time, 8), expected, "t={time}");
        }
    }

    #[test]
    fn six_digit_codes_are_the_low_digits_of_the_same_values() {
        for (time, expected) in VECTORS {
            assert_eq!(code_at(RFC_SECRET, time, 6), expected[2..], "t={time}");
        }
    }

    #[test]
    fn rfc_4226_hotp_vectors() {
        // RFC 4226 appendix D, counter 0..=9, six digits.
        let expected = [
            755224, 287082, 359152, 969429, 338314, 254676, 287922, 162583, 399871, 520489,
        ];
        for (counter, code) in expected.iter().enumerate() {
            assert_eq!(
                hotp(RFC_SECRET, counter as u64, 6),
                *code,
                "counter {counter}"
            );
        }
    }

    #[test]
    fn accepts_the_current_and_adjacent_steps_only() {
        let now = 1_700_000_000;
        let step = step_at(now);
        for offset in [-1i64, 0, 1] {
            let code = code_at(RFC_SECRET, now.wrapping_add_signed(offset * 30), 6);
            assert_eq!(
                matching_step(RFC_SECRET, &code, now),
                Some((step as i64 + offset) as u64)
            );
        }
        for offset in [-2i64, 2, 10] {
            let code = code_at(RFC_SECRET, now.wrapping_add_signed(offset * 30), 6);
            assert_eq!(
                matching_step(RFC_SECRET, &code, now),
                None,
                "offset {offset}"
            );
        }
    }

    #[test]
    fn rejects_malformed_codes() {
        let now = 1_700_000_000;
        let good = code_at(RFC_SECRET, now, 6);
        assert!(matching_step(RFC_SECRET, &good, now).is_some());
        assert!(
            matching_step(RFC_SECRET, &format!(" {good} "), now).is_some(),
            "trimmed"
        );
        for bad in ["", "12345", "1234567", "12345a", "١٢٣٤٥٦", &good[..5]] {
            assert_eq!(matching_step(RFC_SECRET, bad, now), None, "{bad:?}");
        }
    }

    #[test]
    fn base32_round_trips_and_matches_the_rfc_vectors() {
        // RFC 4648 section 10 (without padding).
        for (plain, encoded) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(base32_encode(plain.as_bytes()), encoded);
            assert_eq!(base32_decode(encoded).unwrap(), plain.as_bytes());
        }
        assert_eq!(base32_decode("mzxw 6ytb oi==").unwrap(), b"foobar");
        assert!(base32_decode("MZXW1").is_none(), "1 is not in the alphabet");
        let secret = generate_secret();
        assert_eq!(base32_decode(&base32_encode(&secret)).unwrap(), secret);
    }

    #[test]
    fn secrets_are_random_and_long_enough() {
        let a = generate_secret();
        assert_eq!(a.len(), SECRET_BYTES);
        assert_ne!(a, generate_secret());
    }

    #[test]
    fn otpauth_uri_carries_the_parameters_apps_need() {
        let uri = otpauth_uri("Helix Support", "ann@example.test", b"foobar");
        assert_eq!(
            uri,
            "otpauth://totp/Helix%20Support:ann%40example.test?secret=MZXW6YTBOI&issuer=Helix%20Support&algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn comparison_does_not_short_circuit_on_length_or_content() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"123457"));
        assert!(!constant_time_eq(b"123456", b"12345"));
    }
}
