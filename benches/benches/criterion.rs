use criterion::{criterion_group, criterion_main, Criterion};
use solar_bench::{get_srcs, Source, PARSERS};
use std::time::Duration;

fn parser_benches(c: &mut Criterion) {
    for s in get_srcs() {
        eprintln!("{}: {} LoC, {} bytes", s.name, s.src.lines().count(), s.src.len());
    }
    eprintln!();

    let mut g = c.benchmark_group("parser");
    g.warm_up_time(Duration::from_secs(3));
    g.measurement_time(Duration::from_secs(10));
    g.sample_size(20);
    g.noise_threshold(0.05);

    solar_parse::interface::enter(|| {
        for &Source { name: sname, path: _, src } in get_srcs() {
            for &parser in PARSERS {
                let pname = parser.name();
                let mk_id = |id: &str| {
                    if PARSERS.len() == 1 {
                        format!("{sname}/{id}")
                    } else {
                        format!("{sname}/{pname}/{id}")
                    }
                };
                if parser.can_lex() {
                    g.bench_function(mk_id("lex"), |b| b.iter(|| parser.lex(src)));
                }
                g.bench_function(mk_id("parse"), |b| b.iter(|| parser.parse(src)));
            }
            eprintln!();
        }
    });

    g.finish();
}

criterion_group!(benches, parser_benches);
criterion_main!(benches);
