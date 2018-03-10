pub fn is_valid(hostname: &str) -> bool {
    fn is_alphanumeric(byte: u8) -> bool {
        (byte >= b'a' && byte <= b'z') || (byte >= b'A' && byte <= b'Z')
            || (byte >= b'0' && byte <= b'9') || byte == b'-'
    }

    // Empty hostnames aren't valid
    !hostname.is_empty()
        // First & last characters can't have hyphens
        && !(hostname.starts_with('-') || hostname.ends_with('-'))
        // Only a-zA-Z0-9- is allowed
        && !hostname.bytes().any(|byte| !is_alphanumeric(byte))
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
            "asdf@",
        ] {
            assert!(!is_valid(hostname), "{} should not be valid", hostname);
        }
    }
}
