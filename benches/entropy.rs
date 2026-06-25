use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn entropy_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("entropy");

    let tracking_params = vec![
        "utm_source=twitter&utm_medium=social&utm_campaign=spring2024",
        "fbclid=IwAR0abcdef1234567890abcdef1234567890abcdef1234567890abcdef123456",
        "gclid=CjwKCAiA9v",
        "ref=affiliate&source=newsletter&campaign=spring_sale_2024",
        "sessionid=a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
        "si=a1b2c3d4e5f6g7h8",
    ];

    group.bench_function("shannon_entropy", |b| {
        b.iter(|| {
            for param in &tracking_params {
                let _ = shannon_entropy(black_box(param));
            }
        });
    });

    group.bench_function("url_decode_encode", |b| {
        b.iter(|| {
            for param in &tracking_params {
                let decoded = urlencoding::decode(black_box(param)).unwrap();
                let _ = urlencoding::encode(&decoded);
            }
        });
    });

    group.finish();
}

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

criterion_group!(benches, entropy_benchmark);
criterion_main!(benches);
