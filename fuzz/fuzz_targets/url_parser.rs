#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        if input.len() > 500 || input.len() < 5 {
            return;
        }

        // Test URL parsing
        if let Ok(parsed) = url::Url::parse(input) {
            let _ = parsed.scheme();
            let _ = parsed.host_str();
            let _ = parsed.path();
            let _ = parsed.query();
            let _ = parsed.query_pairs();

            // Test Shannon entropy on query parameters
            if let Some(query) = parsed.query() {
                let entropy = shannon_entropy(query);
                // entropy is a sanity check — should never panic or produce NaN
                let _ = entropy.is_finite();
            }
        }

        // Test percent-encoding roundtrip
        let encoded = percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC);
        let _ = encoded.to_string();

        // Test regex on partial input (regression check)
        let re = regex::Regex::new(
            r"https?://[^\s/$.?#].[^\s]*"
        );
        if let Ok(re) = re {
            let _: Vec<_> = re.find_iter(input).collect();
        }
    }
});

/// Shannon entropy calculation (sync, no dependencies)
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut freq = [0u64; 256];
    let len = s.len() as f64;
    for &b in s.as_bytes() {
        freq[b as usize] += 1;
    }
    -freq
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            p * p.log2()
        })
        .sum::<f64>()
}
