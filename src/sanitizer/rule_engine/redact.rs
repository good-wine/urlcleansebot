use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static SENSITIVE_PATTERNS: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "aws_access_key",
        Regex::new(r"(?i)\b[A-Z0-9]{20}\b").expect("Invalid regex for aws_access_key"),
    );
    m.insert(
        "aws_secret_key",
        Regex::new(r"(?i)\b[A-Za-z0-9/+=]{40}\b").expect("Invalid regex for aws_secret_key"),
    );
    m.insert(
        "password",
        Regex::new(r"(?i)password\s*[:=]\s*[^\s]+").expect("Invalid regex for password"),
    );
    m.insert("ipv4", Regex::new(r"(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)").expect("Invalid regex for ipv4"));
    m.insert(
        "email",
        Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
            .expect("Invalid regex for email"),
    );
    m
});

pub fn redact_sensitive(text: &str) -> String {
    let mut redacted = text.to_string();
    for (name, re) in SENSITIVE_PATTERNS.iter() {
        redacted = re
            .replace_all(&redacted, format!("[REDACTED {}]", name.to_uppercase()))
            .to_string();
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_email() {
        let input = "My email is test@example.com";
        let redacted = redact_sensitive(input);
        assert!(redacted.contains("[REDACTED EMAIL]"));
        assert!(!redacted.contains("test@example.com"));
    }

    #[test]
    fn test_redact_ip() {
        let input = "My IP is 1.2.3.4";
        let redacted = redact_sensitive(input);
        assert!(redacted.contains("[REDACTED IPV4]"));
        assert!(!redacted.contains("1.2.3.4"));
    }
}
