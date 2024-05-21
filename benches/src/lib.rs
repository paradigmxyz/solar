use criterion::Criterion;
use std::{hint::black_box, io::Write, path::PathBuf, process::Stdio, time::Duration};
use sulk_parse::interface::Session;

const PARSERS: &[&dyn Parser] = &[&Solc, &Sulk, &Solang, &Slang];
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
    Source { name: "UniswapV3", src: include_str!("../../testdata/UniswapV3.sol") },
];

#[derive(Clone, Debug)]
struct Source {
    name: &'static str,
    src: &'static str,
}

trait Parser {
    fn name(&self) -> &'static str;
    fn lex(&self, src: &str);
    fn has_lex(&self) -> bool {
        true
    }
    fn parse(&self, src: &str);
}

struct Solc;
impl Parser for Solc {
    fn name(&self) -> &'static str {
        "solc"
    }

    fn has_lex(&self) -> bool {
        false
    }

    fn lex(&self, _: &str) {}

    fn parse(&self, src: &str) {
        let mut cmd = std::process::Command::new("solc");
        cmd.arg("-");
        cmd.arg("--stop-after=parsing");
        // cmd.arg("--ast-compact-json");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());
        let mut child = cmd.spawn().expect("failed to spawn child");
        child.stdin.as_mut().unwrap().write_all(src.as_bytes()).expect("failed to write to stdin");
        let output = child.wait_with_output().expect("failed to wait for child");
        if !output.status.success() {
            panic!("solc failed.\ncmd: {cmd:?}\nout: {output:#?}");
        }
        let _stdout = String::from_utf8(output.stdout).expect("failed to read stdout");
    }
}

struct Sulk;
impl Parser for Sulk {
    fn name(&self) -> &'static str {
        "sulk"
    }

    fn lex(&self, src: &str) {
        let source_map = sulk_parse::interface::SourceMap::empty();
        let sess = Session::with_tty_emitter(source_map.into());
        for token in sulk_parse::Lexer::new(&sess, src) {
            black_box(token);
        }
    }

    fn parse(&self, src: &str) {
        (|| -> sulk_parse::interface::Result {
            let source_map = sulk_parse::interface::SourceMap::empty();
            let sess = Session::with_tty_emitter(source_map.into());
            let filename = PathBuf::from("test.sol");
            let mut parser =
                sulk_parse::Parser::from_source_code(&sess, filename.into(), src.into())?;
            let result = parser.parse_file().map_err(|e| e.emit())?;
            sess.dcx.has_errors()?;
            black_box(result);
            Ok(())
        })()
        .unwrap();
    }
}

struct Solang;
impl Parser for Solang {
    fn name(&self) -> &'static str {
        "solang"
    }

    fn lex(&self, src: &str) {
        for token in solang_parser::lexer::Lexer::new(src, 0, &mut vec![], &mut vec![]) {
            black_box(token);
        }
    }

    fn parse(&self, src: &str) {
        match solang_parser::parse(src, 0) {
            Ok(result) => {
                black_box(result);
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

    fn lex(&self, src: &str) {
        let _ = src;
    }

    fn has_lex(&self) -> bool {
        false
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

pub fn main() {
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

pub fn criterion_main() {
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
                if parser.has_lex() {
                    let id = format!("{name}/{}/lex", parser.name());
                    g.bench_function(id, |b| b.iter(|| parser.lex(src)));
                }
                let id = format!("{name}/{}/parse", parser.name());
                g.bench_function(id, |b| b.iter(|| parser.parse(src)));
            }
        }
    });

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
        sulk_parse::interface::enter(run);
    } else {
        run();
    }
}
