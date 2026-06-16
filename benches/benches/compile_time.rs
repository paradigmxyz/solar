//! Compile-time benchmarks over the synthetic repro corpus.
//!
//! These stress different parts of the compiler (many symbols, deep nesting,
//! etc.) at small/medium/large sizes. The sources and the parse/lower pipeline
//! live in the shared `solar_bench` harness (see `get_repros` and `Solar`);
//! only the criterion wiring lives here.
//!
//! Run with: `cargo bench -p solar-bench --bench compile_time`

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use solar_bench::{Parser, Solar, Source, compiles, get_repros};
use std::{sync::OnceLock, time::Duration};

/// Repros that actually compile, computed once.
///
/// Fixtures that don't compile (e.g. `deep_nesting_large`, which exceeds the
/// parser recursion limit) are skipped with a note rather than panicking inside
/// a timed routine.
fn benchable_repros() -> &'static [&'static Source] {
    static CACHE: OnceLock<Vec<&'static Source>> = OnceLock::new();
    CACHE.get_or_init(|| {
        get_repros()
            .iter()
            .filter(|s| {
                let ok = compiles(s.src);
                if !ok {
                    eprintln!("skipping repro `{}`: does not compile", s.name);
                }
                ok
            })
            .collect()
    })
}

fn make_group<'a>(
    c: &'a mut Criterion,
    name: &str,
) -> criterion::BenchmarkGroup<'a, criterion::measurement::WallTime> {
    let mut g = c.benchmark_group(name);
    g.warm_up_time(Duration::from_secs(2));
    g.measurement_time(Duration::from_secs(5));
    g.sample_size(10);
    g
}

/// Benchmark parsing only.
fn bench_parse(c: &mut Criterion) {
    let mut g = make_group(c, "compile/parse");
    for repro in benchable_repros() {
        let src = repro.src;
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(repro.name), &src, |b, &s| {
            b.iter_batched(
                || Solar.setup(s),
                |mut setup| {
                    Solar.parse(s, &mut *setup);
                    setup
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    g.finish();
}

/// Benchmark the full pipeline (parse + lower to HIR).
fn bench_full(c: &mut Criterion) {
    let mut g = make_group(c, "compile/full");
    for repro in benchable_repros() {
        let src = repro.src;
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(repro.name), &src, |b, &s| {
            b.iter_batched(
                || Solar.setup(s),
                |mut setup| {
                    Solar.lower(s, &mut *setup);
                    setup
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    g.finish();
}

/// Scaling benchmark: full pipeline keyed by line count, to see how compile
/// time grows with input size.
fn bench_scaling(c: &mut Criterion) {
    let mut g = make_group(c, "compile/scaling");
    for repro in benchable_repros() {
        let src = repro.src;
        let lines = src.lines().count();
        g.throughput(Throughput::Elements(lines as u64));
        g.bench_with_input(BenchmarkId::new(repro.name, lines), &src, |b, &s| {
            b.iter_batched(
                || Solar.setup(s),
                |mut setup| {
                    Solar.lower(s, &mut *setup);
                    setup
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    g.finish();
}

criterion_group!(benches, bench_parse, bench_full, bench_scaling);
criterion_main!(benches);
