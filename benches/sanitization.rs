use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn sanitization_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("sanitization");

    // Test URLs with various tracking parameters
    let test_urls = vec![
        "https://example.com/page?utm_source=twitter&utm_medium=social&utm_campaign=spring&foo=bar",
        "https://shop.example.org/products/123?ref=affiliate&source=newsletter&fbclid=IwAR123&gclid=CjwKCAiA9v",
        "https://news.example.com/article/2024/tech?utm_source=facebook&utm_medium=cpc&utm_campaign=launch&utm_term=click&utm_content=banner&from=homepage",
        "https://www.example.co.uk/search?q=rust+programming&source=web&sessionid=abc123&tracking_id=xyz789",
    ];

    group.bench_function("url_parse_only", |b| {
        b.iter(|| {
            for url_str in &test_urls {
                let _ = url::Url::parse(black_box(url_str));
            }
        });
    });

    group.bench_function("query_params_iteration", |b| {
        b.iter(|| {
            for url_str in &test_urls {
                if let Ok(parsed) = url::Url::parse(black_box(url_str)) {
                    if let Some(query) = parsed.query() {
                        let _pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes())
                            .map(|(k, v)| (k.into_owned(), v.into_owned()))
                            .collect();
                    }
                }
            }
        });
    });

    group.bench_function("regex_matching", |b| {
        let re = regex::Regex::new(r"utm_[a-z]+").unwrap();
        b.iter(|| {
            for url_str in &test_urls {
                let _matches: Vec<_> = re.find_iter(black_box(url_str)).collect();
            }
        });
    });

    group.finish();
}

criterion_group!(benches, sanitization_benchmark);
criterion_main!(benches);
