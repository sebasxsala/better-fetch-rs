use better_fetch::{build_url, serialize_to_query_map, QueryValue};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use indexmap::IndexMap;
use std::collections::HashMap;
use url::Url;

fn bench_build_url(c: &mut Criterion) {
    let base = Url::parse("https://api.example.com").unwrap();
    let mut group = c.benchmark_group("build_url");

    for param_count in [1usize, 5, 10] {
        let path = (0..param_count)
            .map(|i| format!(":p{i}"))
            .collect::<Vec<_>>()
            .join("/");
        let path = format!("/items/{path}");
        let mut params = HashMap::new();
        for i in 0..param_count {
            params.insert(format!("p{i}"), format!("value-{i}"));
        }
        let mut query = IndexMap::new();
        query.insert("q".into(), QueryValue::Scalar("rust".into()));
        query.insert(
            "tags".into(),
            QueryValue::Array(vec!["a".into(), "b".into(), "c".into()]),
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(param_count),
            &param_count,
            |b, _| {
                b.iter(|| {
                    black_box(
                        build_url(
                            black_box(&base),
                            black_box(&path),
                            black_box(&params),
                            black_box(&query),
                        )
                        .unwrap(),
                    );
                });
            },
        );
    }
    group.finish();
}

fn bench_parse_embedded_query(c: &mut Criterion) {
    let base = Url::parse("https://api.example.com").unwrap();
    let query_1k = (0..50)
        .map(|i| format!("k{i}=v{i}"))
        .collect::<Vec<_>>()
        .join("&");
    let query_64k = query_1k.repeat(128);

    let mut group = c.benchmark_group("parse_embedded_query");
    for (label, query) in [("1KiB", query_1k.as_str()), ("64KiB", query_64k.as_str())] {
        let path = format!("/search?{query}");
        group.bench_with_input(BenchmarkId::new("embedded", label), &path, |b, path| {
            b.iter(|| {
                black_box(
                    build_url(
                        black_box(&base),
                        black_box(path),
                        black_box(&HashMap::new()),
                        black_box(&IndexMap::new()),
                    )
                    .unwrap(),
                );
            });
        });
    }
    group.finish();
}

#[derive(serde::Serialize)]
struct BenchQuery {
    q: String,
    page: u32,
    active: bool,
    tags: Vec<String>,
}

fn bench_serialize_to_query_map(c: &mut Criterion) {
    let value = BenchQuery {
        q: "better-fetch".into(),
        page: 42,
        active: true,
        tags: (0..8).map(|i| format!("tag-{i}")).collect(),
    };

    c.bench_function("serialize_to_query_map", |b| {
        b.iter(|| black_box(serialize_to_query_map(black_box(&value)).unwrap()));
    });
}

criterion_group!(
    benches,
    bench_build_url,
    bench_parse_embedded_query,
    bench_serialize_to_query_map
);
criterion_main!(benches);
