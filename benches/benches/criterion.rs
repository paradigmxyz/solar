use criterion::{Criterion, criterion_group, criterion_main};
use solar_bench::{PARSERS, Source, get_src, get_srcs};
use std::{hint::black_box, time::Duration};

fn micro_benches(c: &mut Criterion) {
    let mut g = make_group(c, "micro");

    g.bench_function("session/new", |b| {
        b.iter(|| solar_parse::interface::Session::builder().with_stderr_emitter().build());
    });

    {
        let sess =
            &black_box(solar_parse::interface::Session::builder().with_stderr_emitter().build());

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
            solar_parse::interface::SourceMap::default,
            |sm| {
                sm.new_source_file(
                    solar_parse::interface::source_map::FileName::Real(source.path.into()),
                    source.src,
                )
                .unwrap()
            },
            criterion::BatchSize::PerIteration,
        )
    });
}

fn parser_benches(c: &mut Criterion) {
    for s in get_srcs() {
        eprintln!("{}: {} LoC, {} bytes", s.name, s.src.lines().count(), s.src.len());
    }
    eprintln!();

    let mut g = make_group(c, "parser");

    for &Source { name: sname, path: _, src, capabilities: ref scaps } in get_srcs() {
        for &parser in PARSERS {
            let pname = parser.name();

            let mk_id = |id: &str| {
                if PARSERS.len() == 1 {
                    format!("{sname}/{id}")
                } else {
                    format!("{sname}/{pname}/{id}")
                }
            };
            let setup = &mut *parser.setup(src);
            if parser.capabilities().can_lex() {
                g.bench_function(mk_id("lex"), |b| b.iter(|| parser.lex(src, setup)));
            }
            g.bench_function(mk_id("parse"), |b| b.iter(|| parser.parse(src, setup)));
            if parser.capabilities().can_lower() && scaps.can_lower() {
                g.bench_function(mk_id("lower"), |b| b.iter(|| parser.lower(src, setup)));
            }
        }
        eprintln!();
    }

    g.finish();
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

criterion_group!(benches, micro_benches, parser_benches);
criterion_main!(benches);
