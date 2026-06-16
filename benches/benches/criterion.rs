use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use solar_bench::{COMPILERS, Compiler, Source, get_src, get_srcs};
use std::{any::Any, hint::black_box, time::Duration};

type CompilerBench = (
    &'static str,
    fn(&dyn Compiler, &Source) -> bool,
    fn(&Source) -> Throughput,
    fn(&dyn Compiler, &Source, &mut dyn Any),
);

fn micro_benches(c: &mut Criterion) {
    let mut g = make_group(c, "micro");

    g.bench_function("session/new", |b| {
        b.iter(|| solar::parse::interface::Session::builder().with_stderr_emitter().build());
    });

    {
        let sess =
            &black_box(solar::parse::interface::Session::builder().with_stderr_emitter().build());

        g.bench_function("session/enter", |b| {
            b.iter(|| black_box(sess).enter(|| black_box(sess)));
        });
        g.bench_function("session/enter_sequential", |b| {
            let n: usize = black_box(10_000);
            b.iter(|| {
                for _ in 0..n {
                    black_box(sess).enter_sequential(|| black_box(sess));
                }
            });
        });

        g.bench_function("session/enter/reentrant", |b| {
            sess.enter(|| {
                let n: usize = black_box(10_000);
                b.iter(|| {
                    for _ in 0..n {
                        black_box(sess).enter(|| black_box(sess));
                    }
                });
            });
        });
        g.bench_function("session/enter_sequential/reentrant", |b| {
            sess.enter(|| {
                let n: usize = black_box(10_000);
                b.iter(|| {
                    for _ in 0..n {
                        black_box(sess).enter_sequential(|| black_box(sess));
                    }
                });
            });
        });
    }

    g.bench_function("source_map/new_source_file", |b| {
        let source = black_box(get_src("Optimism"));
        b.iter_batched_ref(
            solar::parse::interface::SourceMap::default,
            |sm| {
                sm.new_source_file(
                    solar::parse::interface::source_map::FileName::Real(source.path.into()),
                    source.src,
                )
                .unwrap()
            },
            criterion::BatchSize::PerIteration,
        )
    });
}

fn compiler_benches(c: &mut Criterion) {
    for s in get_srcs() {
        eprintln!("{}: {} LoC, {} bytes", s.name, s.src.lines().count(), s.src.len());
    }
    eprintln!();

    let mut g = make_group(c, "parser");
    let benches: [CompilerBench; 4] = [
        ("lex", can_lex, bytes, run_lex),
        ("parse", can_parse, bytes, run_parse),
        ("lower", can_lower, bytes, run_lower),
        ("codegen", can_codegen, bytes, run_codegen),
    ];

    for source in get_srcs() {
        for &compiler in COMPILERS {
            let cname = compiler.name();

            let mk_id = |id: &str| {
                if COMPILERS.len() == 1 {
                    format!("{}/{id}", source.name)
                } else {
                    format!("{}/{cname}/{id}", source.name)
                }
            };
            for (name, should_run, throughput, run) in benches {
                if should_run(compiler, source) {
                    g.throughput(throughput(source));
                    g.bench_function(mk_id(name), |b| {
                        b.iter_batched(
                            || compiler.setup(source),
                            |mut setup| {
                                run(compiler, source, &mut *setup);
                                setup
                            },
                            criterion::BatchSize::SmallInput,
                        )
                    });
                }
            }
        }
        eprintln!();
    }

    g.finish();
}

fn bytes(source: &Source) -> Throughput {
    Throughput::Bytes(source.src.len() as u64)
}

fn can_lex(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.capabilities().can_lex() && source.capabilities.can_lex()
}

fn can_parse(compiler: &dyn Compiler, source: &Source) -> bool {
    let _ = (compiler, source);
    // compiler.capabilities().can_parse() && source.capabilities.can_parse()
    true
}

fn can_lower(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.capabilities().can_lower() && source.capabilities.can_lower()
}

fn can_codegen(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.capabilities().can_codegen() && source.capabilities.can_codegen()
}

fn run_lex(compiler: &dyn Compiler, source: &Source, setup: &mut dyn Any) {
    compiler.lex(source, setup);
}

fn run_parse(compiler: &dyn Compiler, source: &Source, setup: &mut dyn Any) {
    compiler.parse(source, setup);
}

fn run_lower(compiler: &dyn Compiler, source: &Source, setup: &mut dyn Any) {
    compiler.lower(source, setup);
}

fn run_codegen(compiler: &dyn Compiler, source: &Source, setup: &mut dyn Any) {
    compiler.codegen(source, setup);
}

fn make_group<'a>(
    c: &'a mut Criterion,
    name: &str,
) -> criterion::BenchmarkGroup<'a, criterion::measurement::WallTime> {
    let mut g = c.benchmark_group(name);
    g.warm_up_time(Duration::from_secs(3));
    g.measurement_time(Duration::from_secs(10));
    g.sample_size(10);
    g.noise_threshold(0.05);
    g
}

criterion_group!(benches, micro_benches, compiler_benches);
criterion_main!(benches);
