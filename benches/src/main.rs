use criterion::Criterion;
use std::{hint::black_box, path::PathBuf, time::Duration};
use sulk_parse::ParseSess;

const PARSERS: &[&dyn Parser] = &[&Sulk, &Solang, &Slang];
const SRCS: &[Source] = &[
    Source { name: "empty", src: "" },
    Source {
        name: "simple",
        src: r#"
        pragma solidity ^0.8.0;

        contract A {
            function f() public pure returns (uint) {
                return 1;
            }
        }
        "#,
    },
    Source {
        name: "verifier",
        src: include_str!("../../testdata/solidity/test/benchmarks/verifier.sol"),
    },
    Source {
        name: "OptimizorClub",
        src: include_str!("../../testdata/solidity/test/benchmarks/OptimizorClub.sol"),
    },
];

#[derive(Clone, Debug)]
struct Source {
    name: &'static str,
    src: &'static str,
}

trait Parser {
    fn name(&self) -> &'static str;
    fn parse(&self, s: &str);
}

struct Sulk;
impl Parser for Sulk {
    fn name(&self) -> &'static str {
        "sulk"
    }

    fn parse(&self, src: &str) {
        (|| {
            let source_map = sulk_parse::interface::SourceMap::empty();
            let sess = ParseSess::with_tty_emitter(source_map.into());
            let filename = PathBuf::from("test.sol");
            let mut parser =
                sulk_parse::Parser::from_source_code(&sess, filename.into(), src.into());
            let f = parser.parse_file().map_err(|e| e.emit())?;
            sess.dcx.has_errors()?;
            black_box(f);
            Ok::<_, sulk_parse::interface::diagnostics::ErrorGuaranteed>(())
        })()
        .unwrap();
    }
}

struct Solang;
impl Parser for Solang {
    fn name(&self) -> &'static str {
        "solang"
    }

    fn parse(&self, src: &str) {
        match solang_parser::parse(src, 0) {
            Ok(res) => {
                black_box(res);
            }
            Err(diagnostics) => {
                if !diagnostics.is_empty() {
                    for diagnostic in diagnostics {
                        eprintln!("{diagnostic:?}");
                    }
                    panic!();
                }
            }
        }
    }
}

struct Slang;
impl Parser for Slang {
    fn name(&self) -> &'static str {
        "slang"
    }

    fn parse(&self, src: &str) {
        let version = semver::Version::new(0, 8, 22);
        let lang = slang_solidity::language::Language::new(version).unwrap();
        let rule = slang_solidity::kinds::RuleKind::SourceUnit;
        let output = lang.parse(rule, src);

        let errors = output.errors();
        if !errors.is_empty() {
            for err in errors {
                let e = err.to_error_report("test.sol", src, true);
                eprintln!("{e}");
            }
            panic!();
        }

        let res = output.tree();
        black_box(res);
    }
}

fn main() {
    if false {
        let path = std::env::args().nth(1).unwrap();
        let s = std::fs::read_to_string(path).unwrap();
        Slang.parse(&s);
        return;
    }

    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut valgrind = false;
    let mut criterion = false;
    args.retain(|arg| match arg.as_str() {
        "--valgrind" => {
            valgrind = true;
            false
        }
        "--bench" => {
            criterion = true;
            true
        }
        "--help" => {
            if valgrind {
                let exe = std::env::args().next().unwrap();
                eprintln!("Usage: {exe} --valgrind [--parser=PARSER] [benchmarks...]");
                std::process::exit(0);
            }
            true
        }
        _ => true,
    });

    match (valgrind, criterion) {
        (true, true) => {
            eprintln!("--valgrind and --bench are mutually exclusive");
            std::process::exit(1);
        }
        (false, false) => {
            eprintln!("must set at least one of --valgrind or --bench");
            std::process::exit(1);
        }
        (true, false) => valgrind_main(&args),
        (false, true) => criterion_main(),
    }
}

fn criterion_main() {
    criterion::criterion_group!(benches, criterion_benches);
    criterion::criterion_main!(benches);
    main();
}

fn criterion_benches(c: &mut Criterion) {
    let mut g = c.benchmark_group("parser");
    g.warm_up_time(Duration::from_secs(5));
    g.measurement_time(Duration::from_secs(10));
    g.sample_size(50);
    g.noise_threshold(0.05);

    sulk_parse::interface::enter(|| {
        for &Source { name, src } in SRCS {
            for &parser in PARSERS {
                let id = format!("{name}/{}", parser.name());
                g.bench_function(id, |b| b.iter(|| parser.parse(src)));
            }
        }
    })
    .unwrap();

    g.finish();
}

fn valgrind_main(args: &[String]) {
    let mut benches = Vec::<&'static Source>::new();
    let mut parsers = Vec::<&'static dyn Parser>::new();
    let mut has_sulk = false;
    for arg in args.iter() {
        if arg.starts_with("--parser=") {
            continue;
        }
        if let Some(src) = SRCS.iter().find(|s| s.name == arg) {
            benches.push(src);
        }
        if let Some(src) = PARSERS.iter().find(|p| p.name() == arg) {
            if src.name() == "sulk" {
                has_sulk = true;
            }
            parsers.push(*src);
        }
    }
    if benches.is_empty() {
        benches = SRCS.iter().collect();
    }
    if parsers.is_empty() {
        has_sulk = true;
        parsers = PARSERS.to_vec();
    }

    let run = || {
        for &&Source { name, src } in &benches {
            for &parser in &parsers {
                println!("running {name}/{}", parser.name());
                parser.parse(src);
            }
        }
    };
    if has_sulk {
        sulk_parse::interface::enter(run).unwrap();
    } else {
        run();
    }
}
