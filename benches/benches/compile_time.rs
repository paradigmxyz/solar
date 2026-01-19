//! Compile-time benchmarks for profiling HIR/MIR lowering and codegen.
//!
//! This benchmarks synthetic Solidity files that stress different parts of the compiler.
//! Run with: `cargo bench -p solar-bench --bench compile_time`

#![allow(clippy::disallowed_methods)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use solar::{
    parse::{ast::Arena, interface::Session},
    sema::Compiler,
};
use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    time::Duration,
};

/// Repro categories for benchmarking.
#[derive(Clone, Copy, Debug)]
struct ReproConfig {
    name: &'static str,
    pattern: &'static str,
}

const REPRO_CONFIGS: &[ReproConfig] = &[
    ReproConfig { name: "many_symbols", pattern: "many_symbols" },
    ReproConfig { name: "many_functions", pattern: "many_functions" },
    ReproConfig { name: "deep_nesting", pattern: "deep_nesting" },
    ReproConfig { name: "many_types", pattern: "many_types" },
    ReproConfig { name: "large_literals", pattern: "large_literals" },
    ReproConfig { name: "many_storage", pattern: "many_storage" },
    ReproConfig { name: "many_events", pattern: "many_events" },
    ReproConfig { name: "complex_inheritance", pattern: "complex_inheritance" },
    ReproConfig { name: "many_mappings", pattern: "many_mappings" },
    ReproConfig { name: "many_modifiers", pattern: "many_modifiers" },
];

const SIZES: &[&str] = &["small", "medium", "large"];

fn get_repro_path(config: &ReproConfig, size: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("testdata/repros")
        .join(format!("{}_{}.sol", config.pattern, size))
}

fn read_repro(config: &ReproConfig, size: &str) -> Option<String> {
    let path = get_repro_path(config, size);
    fs::read_to_string(&path).ok()
}

fn session() -> Session {
    Session::builder()
        .with_stderr_emitter_and_color(solar::parse::interface::ColorChoice::Never)
        .single_threaded()
        .build()
}

/// Benchmark parsing phase only.
fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("compile/parse");
    g.warm_up_time(Duration::from_secs(2));
    g.measurement_time(Duration::from_secs(5));
    g.sample_size(10);

    for config in REPRO_CONFIGS {
        for size in SIZES {
            let Some(src) = read_repro(config, size) else {
                continue;
            };
            let src: &'static str = Box::leak(src.into_boxed_str());
            let id = BenchmarkId::new(config.name, size);

            g.throughput(Throughput::Bytes(src.len() as u64));
            g.bench_with_input(id, &src, |b, &src| {
                let sess = session();
                b.iter(|| {
                    sess.enter(|| {
                        let arena = Arena::new();
                        let mut parser = solar::parse::Parser::from_source_code(
                            &sess,
                            &arena,
                            PathBuf::from("test.sol").into(),
                            src,
                        )
                        .unwrap();
                        let result = parser.parse_file().unwrap();
                        black_box(result);
                    });
                });
            });
        }
    }
    g.finish();
}

/// Benchmark semantic analysis (AST lowering to HIR).
fn bench_sema(c: &mut Criterion) {
    let mut g = c.benchmark_group("compile/sema");
    g.warm_up_time(Duration::from_secs(2));
    g.measurement_time(Duration::from_secs(5));
    g.sample_size(10);

    for config in REPRO_CONFIGS {
        for size in SIZES {
            let Some(src) = read_repro(config, size) else {
                continue;
            };
            let src: &'static str = Box::leak(src.into_boxed_str());
            let id = BenchmarkId::new(config.name, size);

            g.throughput(Throughput::Bytes(src.len() as u64));
            g.bench_with_input(id, &src, |b, &src| {
                b.iter_batched(
                    || Compiler::new(session()),
                    |mut compiler| {
                        compiler.enter_mut(|compiler| {
                            let mut parsing = compiler.parse();
                            parsing.add_file(
                                compiler
                                    .sess()
                                    .source_map()
                                    .new_source_file(PathBuf::from("test.sol"), src)
                                    .unwrap(),
                            );
                            parsing.parse();
                            let _ = compiler.lower_asts();
                        });
                        black_box(compiler)
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        }
    }
    g.finish();
}

/// Benchmark full compilation pipeline (parse + sema + lower).
fn bench_full(c: &mut Criterion) {
    let mut g = c.benchmark_group("compile/full");
    g.warm_up_time(Duration::from_secs(2));
    g.measurement_time(Duration::from_secs(5));
    g.sample_size(10);

    for config in REPRO_CONFIGS {
        for size in SIZES {
            let Some(src) = read_repro(config, size) else {
                continue;
            };
            let src: &'static str = Box::leak(src.into_boxed_str());
            let id = BenchmarkId::new(config.name, size);

            g.throughput(Throughput::Bytes(src.len() as u64));
            g.bench_with_input(id, &src, |b, &src| {
                b.iter_batched(
                    || Compiler::new(session()),
                    |mut compiler| {
                        compiler.enter_mut(|compiler| {
                            let mut parsing = compiler.parse();
                            parsing.add_file(
                                compiler
                                    .sess()
                                    .source_map()
                                    .new_source_file(PathBuf::from("test.sol"), src)
                                    .unwrap(),
                            );
                            parsing.parse();
                            let _ = compiler.lower_asts();
                        });
                        black_box(compiler)
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        }
    }
    g.finish();
}

/// Scaling benchmark - how compile time grows with input size.
fn bench_scaling(c: &mut Criterion) {
    let mut g = c.benchmark_group("compile/scaling");
    g.warm_up_time(Duration::from_secs(1));
    g.measurement_time(Duration::from_secs(3));
    g.sample_size(10);

    // Collect all sizes for each config type
    for config in REPRO_CONFIGS {
        for size in SIZES {
            let Some(src) = read_repro(config, size) else {
                continue;
            };
            let src: &'static str = Box::leak(src.into_boxed_str());
            let lines = src.lines().count();
            let id = BenchmarkId::new(format!("{}/{}", config.name, size), lines);

            g.throughput(Throughput::Elements(lines as u64));
            g.bench_with_input(id, &src, |b, &src| {
                b.iter_batched(
                    || Compiler::new(session()),
                    |mut compiler| {
                        compiler.enter_mut(|compiler| {
                            let mut parsing = compiler.parse();
                            parsing.add_file(
                                compiler
                                    .sess()
                                    .source_map()
                                    .new_source_file(PathBuf::from("test.sol"), src)
                                    .unwrap(),
                            );
                            parsing.parse();
                            let _ = compiler.lower_asts();
                        });
                        black_box(compiler)
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        }
    }
    g.finish();
}

criterion_group!(benches, bench_parse, bench_sema, bench_full, bench_scaling);
criterion_main!(benches);
