use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use sulk_bench::{Source, PARSERS, SRCS};

fn criterion_benches(c: &mut Criterion) {
    let mut g = c.benchmark_group("parser");
    g.warm_up_time(Duration::from_secs(2));
    g.measurement_time(Duration::from_secs(5));
    g.sample_size(50);
    g.noise_threshold(0.05);

    sulk_parse::interface::enter(|| {
        for &Source { name: sname, src } in SRCS {
            for &parser in PARSERS {
                let pname = parser.name();
                if parser.can_lex() {
                    let id = format!("{sname}/{pname}/lex");
                    g.bench_function(id, |b| b.iter(|| parser.lex(src)));
                }
                let id = format!("{sname}/{pname}/parse");
                g.bench_function(id, |b| b.iter(|| parser.parse(src)));
            }
        }
    });

    g.finish();
}

criterion_group!(benches, criterion_benches);
criterion_main!(benches);
