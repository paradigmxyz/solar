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
                if parser.can_lex() {
                    let id = format!("{sname}/{pname}/lex");
                    g.bench_function(id, |b| b.iter(|| parser.lex(src)));
                }
                let id = format!("{sname}/{pname}/parse");
                g.bench_function(id, |b| b.iter(|| parser.parse(src)));
            }
            eprintln!();
        }
    });

    g.finish();
}

criterion_group!(benches, parser_benches);
criterion_main!(benches);
