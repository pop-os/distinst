#![no_std]

//! Validate a hostname according to [IETF RFC 1123](https://tools.ietf.org/html/rfc1123).
//!
//! ```rust
//! extern crate hostname_validator;
//! 
//! let valid = "VaLiD-HoStNaMe";
//! let invalid = "-invalid-name";
//! 
//! assert!(hostname_validator::is_valid(valid));
//! assert!(!hostname_validator::is_valid(invalid));
//! ```

/// Validate a hostname according to [IETF RFC 1123](https://tools.ietf.org/html/rfc1123).
///
/// A hostname is valid if the following condition are true:
///
/// - It does not start or end with `-`.
/// - It does not contain any characters outside of the alphanumeric range, except for `-`.
/// - It is not empty.
pub fn is_valid(hostname: &str) -> bool {
    fn is_alphanumeric(byte: u8) -> bool {
        (byte >= b'a' && byte <= b'z') || (byte >= b'A' && byte <= b'Z')
            || (byte >= b'0' && byte <= b'9') || byte == b'-'
    }

    !(hostname.bytes().any(|byte| !is_alphanumeric(byte))
        || hostname.ends_with('-')
        || hostname.starts_with('-')
        || hostname.is_empty()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_hostnames() {
        for hostname in &["VaLiD-HoStNaMe", "50-name", "235235"] {
            assert!(is_valid(hostname), "{} is not valid", hostname);
        }
    }

    #[test]
    fn invalid_hostnames() {
        for hostname in &[
            "-invalid-name",
            "also-invalid-",
            "asdf@fasd",
            "@asdfl",
            "asd f@",
        ] {
            assert!(!is_valid(hostname), "{} should not be valid", hostname);
        }
    }
}
