use solar_parse::interface::Session;
use std::{
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
};

#[allow(unexpected_cfgs)]
pub const PARSERS: &[&dyn Parser] =
    if cfg!(codspeed) { &[&Solar] } else { &[&Solc, &Solar, &Solang, &Slang, &TreeSitter] };

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
            include_source("../testdata/Seaport.sol"),
            include_source("../testdata/Solady.sol"),
            include_source("../testdata/Optimism.sol"),
        ]
    })
}

fn include_source(path: &'static str) -> Source {
    let source = match std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)) {
        Ok(source) => source,
        Err(e) => panic!(
            "failed to read {path}: {e};\n\
             you may need to initialize submodules: `git submodule update --init --checkout`"
        ),
    };
    source_from_path(path, source.leak())
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
        let sess = session();
        for token in solar_parse::Lexer::new(&sess, src) {
            black_box(token);
        }
        sess.dcx.has_errors().unwrap();
    }

    fn parse(&self, src: &str) {
        let sess = session();
        sess.enter(|| -> solar_parse::interface::Result {
            let arena = solar_parse::ast::Arena::new();
            let filename = PathBuf::from("test.sol");
            let mut parser =
                solar_parse::Parser::from_source_code(&sess, &arena, filename.into(), src)?;
            let result = parser.parse_file().map_err(|e| e.emit())?;
            sess.dcx.has_errors()?;
            black_box(result);
            Ok(())
        })
        .unwrap();
    }
}

fn session() -> Session {
    Session::builder()
        .with_stderr_emitter_and_color(solar_parse::interface::ColorChoice::Always)
        .single_threaded()
        .build()
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
                let range = err.text_range();
                let slice = src.get(range.start.utf8..range.end.utf8).unwrap_or("<invalid range>");
                let line_col =
                    |i: &slang_solidity::cst::TextIndex| format!("{}:{}", i.line + 1, i.column + 1);
                eprintln!(
                    "{}: {}: {err} @ {slice:?}",
                    line_col(&range.start),
                    line_col(&range.end),
                );
            }
            panic!();
        }

        let res = output.tree();
        black_box(res);
    }
}

pub struct TreeSitter;
impl Parser for TreeSitter {
    fn name(&self) -> &'static str {
        "tree-sitter"
    }

    fn lex(&self, src: &str) {
        let _ = src;
    }

    fn can_lex(&self) -> bool {
        false
    }

    fn parse(&self, src: &str) {
        #[cold]
        #[inline(never)]
        fn on_error(src: &str, tree: &tree_sitter::Tree) -> ! {
            tree.print_dot_graph(&std::fs::File::create("tree.dot").unwrap());

            let mut msg = String::new();
            let mut cursor = tree.walk();
            let root = tree.root_node();
            let mut q = vec![root];
            while let Some(node) = q.pop() {
                if node != root && node.is_error() {
                    let src = &src[node.byte_range()];
                    msg.push_str(&format!("  - {node:?} -> {src:?}\n"));
                }
                q.extend(node.children(&mut cursor));
            }

            panic!("tree-sitter parser failed; dumped to tree.dot\n{msg}");
        }

        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_solidity::LANGUAGE;
        parser.set_language(&language.into()).expect("Error loading Solidity parser");
        let tree = parser.parse(src, None).unwrap();
        if tree.root_node().has_error() {
            on_error(src, &tree);
        }
    }
}
