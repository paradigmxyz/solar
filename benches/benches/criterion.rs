use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use solar_bench::{COMPILERS, Compiler, IS_CODSPEED, Source, get_src, get_srcs};
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
        let (name, content) = &source.files[0];
        b.iter_batched_ref(
            solar::parse::interface::SourceMap::default,
            |sm| {
                sm.new_source_file(
                    solar::parse::interface::source_map::FileName::Real(name.as_ref().into()),
                    content.to_string(),
                )
                .unwrap()
            },
            criterion::BatchSize::PerIteration,
        )
    });

    for (name, source) in [
        ("ascii", "contract C { function f() external {} }\n".repeat(256)),
        ("unicode", "contract C { function f() external {} // \u{03b2}\n".repeat(256)),
    ] {
        let sm = solar::parse::interface::SourceMap::default();
        let file = sm
            .new_source_file(
                solar::parse::interface::source_map::FileName::Custom(name.into()),
                source,
            )
            .unwrap();
        let positions =
            file.src.char_indices().step_by(17).map(|(offset, _)| offset).collect::<Vec<_>>();
        g.bench_function(format!("source_map/lookup_char_pos/{name}"), |b| {
            b.iter(|| {
                let mut total = 0usize;
                for &offset in &positions {
                    let loc = sm.lookup_char_pos(solar::parse::interface::BytePos(offset as u32));
                    total = total.wrapping_add(loc.data.col.0);
                }
                black_box(total)
            });
        });
    }
}

fn compiler_benches(c: &mut Criterion) {
    for s in get_srcs() {
        let lines = s.files.iter().map(|(_, content)| content.lines().count()).sum::<usize>();
        eprintln!("{}: {} files, {} LoC, {} bytes", s.name, s.files.len(), lines, s.bytes);
    }
    eprintln!();

    let mut g = make_group(c, "compiler");
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
    Throughput::Bytes(source.bytes)
}

fn can_lex(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.supports(source) && compiler.capabilities().can_lex() && source.capabilities.can_lex()
}

fn can_parse(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.supports(source)
        && compiler.capabilities().can_parse()
        && source.capabilities.can_parse()
}

fn can_lower(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.supports(source)
        && compiler.capabilities().can_lower()
        && source.capabilities.can_lower()
}

fn can_codegen(compiler: &dyn Compiler, source: &Source) -> bool {
    compiler.supports(source)
        && compiler.capabilities().can_codegen()
        && source.capabilities.can_codegen()
        && (!IS_CODSPEED || source.codspeed_codegen)
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
