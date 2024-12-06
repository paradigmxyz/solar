use solar_parse::interface::Session;
use std::{
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
};

pub const PARSERS: &[&dyn Parser] = &[&Solc, &Solar, &Solang, &Slang];

pub fn get_srcs() -> &'static [Source] {
    static CACHE: std::sync::OnceLock<Vec<Source>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        vec![
            Source { name: "empty", path: "", src: "" },
            include_source("../testdata/Counter.sol"),
            include_source("../testdata/solidity/test/benchmarks/verifier.sol"),
            include_source("../testdata/solidity/test/benchmarks/OptimizorClub.sol"),
            include_source("../testdata/UniswapV3.sol"),
            include_source("../testdata/Solarray.sol"),
            include_source("../testdata/console.sol"),
            include_source("../testdata/Vm.sol"),
            include_source("../testdata/safeconsole.sol"),
        ]
    })
}

fn include_source(path: &'static str) -> Source {
    source_from_path(
        path,
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|e| {
                panic!("failed to read {path}: {e}; you may need to initialize submodules")
            })
            .leak(),
    )
}

fn source_from_path(path: &'static str, src: &'static str) -> Source {
    Source { name: Path::new(path).file_stem().unwrap().to_str().unwrap(), path, src }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: &'static str,
    pub path: &'static str,
    pub src: &'static str,
}

pub trait Parser {
    fn name(&self) -> &'static str;
    fn lex(&self, src: &str);
    fn can_lex(&self) -> bool {
        true
    }
    fn parse(&self, src: &str);
}

pub struct Solc;
impl Parser for Solc {
    fn name(&self) -> &'static str {
        "solc"
    }

    fn can_lex(&self) -> bool {
        false
    }

    fn lex(&self, _: &str) {}

    fn parse(&self, src: &str) {
        let solc = std::env::var_os("SOLC");
        let solc = solc.as_deref().unwrap_or_else(|| "solc".as_ref());
        let mut cmd = std::process::Command::new(solc);
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

pub struct Solar;
impl Parser for Solar {
    fn name(&self) -> &'static str {
        "solar"
    }

    fn lex(&self, src: &str) {
        let sess = Session::builder().with_stderr_emitter().build();
        for token in solar_parse::Lexer::new(&sess, src) {
            black_box(token);
        }
        sess.dcx.has_errors().unwrap();
    }

    fn parse(&self, src: &str) {
        let sess = Session::builder().with_stderr_emitter().build();
        sess.enter(|| -> solar_parse::interface::Result {
            let arena = solar_parse::ast::Arena::new();
            let filename = PathBuf::from("test.sol");
            let mut parser =
                solar_parse::Parser::from_source_code(&sess, &arena, filename.into(), src.into())?;
            let result = parser.parse_file().map_err(|e| e.emit())?;
            sess.dcx.has_errors()?;
            black_box(result);
            Ok(())
        })
        .unwrap();
    }
}

pub struct Solang;
impl Parser for Solang {
    fn name(&self) -> &'static str {
        "solang"
    }

    fn lex(&self, src: &str) {
        let mut comments = vec![];
        let mut errors = vec![];
        for token in solang_parser::lexer::Lexer::new(src, 0, &mut comments, &mut errors) {
            black_box(token);
        }

        if !errors.is_empty() {
            for error in errors {
                eprintln!("{error:?}");
            }
            panic!();
        }

        black_box(comments);
        black_box(errors);
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

pub struct Slang;
impl Parser for Slang {
    fn name(&self) -> &'static str {
        "slang"
    }

    fn lex(&self, src: &str) {
        let _ = src;
    }

    fn can_lex(&self) -> bool {
        false
    }

    fn parse(&self, src: &str) {
        let version = semver::Version::new(0, 8, 22);
        let parser = slang_solidity::parser::Parser::create(version).unwrap();
        let rule = slang_solidity::cst::NonterminalKind::SourceUnit;
        let output = parser.parse(rule, src);

        let errors = output.errors();
        if !errors.is_empty() {
            for err in errors {
                eprintln!("{err}");
            }
            panic!();
        }

        let res = output.tree();
        black_box(res);
    }
}
