#![allow(unused_crate_dependencies)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use solar_lsp::benchmark_diagnostic_conversion;
use std::hint::black_box;

const OPTIMISM_SOURCE: &str = include_str!("../../../testdata/Optimism.sol");

fn diagnostic_conversion(c: &mut Criterion) {
    let source = OPTIMISM_SOURCE.to_owned();
    let mut group = c.benchmark_group("lsp/diagnostic-conversion");
    group.throughput(Throughput::Bytes(source.len() as u64));
    for diagnostic_count in [1, 16, 64] {
        for cached in [false, true] {
            let name = format!(
                "{diagnostic_count}-diagnostics-{}",
                if cached { "cached" } else { "uncached" }
            );
            assert_eq!(
                benchmark_diagnostic_conversion(source.clone(), diagnostic_count, cached),
                diagnostic_count
            );
            group.bench_function(BenchmarkId::from_parameter(name), |b| {
                b.iter(|| {
                    black_box(benchmark_diagnostic_conversion(
                        black_box(source.clone()),
                        diagnostic_count,
                        cached,
                    ))
                });
            });
        }
    }
    group.finish();
}

criterion_group!(benches, diagnostic_conversion);
criterion_main!(benches);
