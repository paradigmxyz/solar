#![allow(clippy::disallowed_methods)]

use solar::{parse::interface::Session, sema::Compiler};
use std::{
    any::Any,
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
};

#[allow(unexpected_cfgs)]
pub const PARSERS: &[&dyn Parser] = if true {
    // Only benchmark our own code in CI.
    &[&Solar]
} else {
    &[
        // fmt
        &Solc,
        &Solar,
        &Solang,
        &Slang,
        &TreeSitter,
    ]
};

pub fn get_srcs() -> &'static [Source] {
    // Please do not modify the order of the sources and only add new sources at the end.
    static CACHE: std::sync::OnceLock<Vec<Source>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        vec![
            Source { name: "empty", path: "", src: "", capabilities: Capabilities::all() },
            include_source("../testdata/Counter.sol", Capabilities::all()),
            include_source(
                "../testdata/solidity/test/benchmarks/verifier.sol",
                Capabilities::all(),
            ),
            include_source(
                "../testdata/solidity/test/benchmarks/OptimizorClub.sol",
                Capabilities::all(),
            ),
            include_source("../testdata/UniswapV3.sol", Capabilities::all()),
            include_source("../testdata/Solarray.sol", Capabilities::all()),
            include_source("../testdata/console.sol", Capabilities::all()),
            include_source("../testdata/Vm.sol", Capabilities::all()),
            include_source("../testdata/safeconsole.sol", Capabilities::all()),
            include_source("../testdata/Seaport.sol", Capabilities::all()),
            include_source("../testdata/Solady.sol", Capabilities::all()),
            include_source("../testdata/Optimism.sol", Capabilities::lex_and_parse()),
        ]
    })
}

pub fn get_src(name: &str) -> &'static Source {
    get_srcs().iter().find(|s| s.name == name).unwrap()
}

/// `include!` at runtime, since the submodule may not be initialized.
fn include_source(path: &'static str, capabilities: Capabilities) -> Source {
    let source = match std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)) {
        Ok(source) => source,
        Err(e) => panic!(
            "failed to read {path}: {e};\n\
             you may need to initialize submodules: `git submodule update --init --checkout`"
        ),
    };
    source_from_path(path, source.leak(), capabilities)
}

fn source_from_path(path: &'static str, src: &'static str, capabilities: Capabilities) -> Source {
    Source { name: Path::new(path).file_stem().unwrap().to_str().unwrap(), path, src, capabilities }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: &'static str,
    pub path: &'static str,
    pub src: &'static str,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    lex: bool,
    lower: bool,
}

impl Capabilities {
    pub fn all() -> Self {
        Self { lex: true, lower: true }
    }

    pub fn parse_only() -> Self {
        Self { lex: false, lower: false }
    }

    pub fn lex_and_parse() -> Self {
        Self { lex: true, lower: false }
    }

    pub fn can_lex(&self) -> bool {
        self.lex
    }

    pub fn can_lower(&self) -> bool {
        self.lower
    }
}

pub trait Parser {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn setup(&self, _src: &str) -> Box<dyn Any> {
        Box::new(())
    }
    fn lex(&self, _src: &str, _setup: &mut dyn Any) {}
    fn parse(&self, src: &str, setup: &mut dyn Any);
    fn lower(&self, _src: &str, _setup: &mut dyn Any) {}
}

pub struct Solc;
impl Parser for Solc {
    fn name(&self) -> &'static str {
        "solc"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, src: &str, _: &mut dyn Any) {
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

    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    fn setup(&self, _src: &str) -> Box<dyn Any> {
        Box::new(Compiler::new(session()))
    }

    fn lex(&self, src: &str, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_ref::<Compiler>().unwrap();
        compiler.enter(|compiler| {
            for token in solar::parse::Lexer::new(compiler.sess(), src) {
                black_box(token);
            }
            compiler.dcx().has_errors().unwrap();
        });
    }

    fn parse(&self, src: &str, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<Compiler>().unwrap();
        compiler
            .enter_mut(|compiler| -> solar::parse::interface::Result {
                let arena = solar::parse::ast::Arena::new();
                let filename = PathBuf::from("test.sol");
                let mut parser = solar::parse::Parser::from_source_code(
                    compiler.sess(),
                    &arena,
                    filename.into(),
                    src,
                )?;
                let result = parser.parse_file().map_err(|e| e.emit())?;
                compiler.dcx().has_errors()?;
                black_box(result);
                Ok(())
            })
            .unwrap();
    }

    fn lower(&self, src: &str, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<Compiler>().unwrap();
        compiler.enter_mut(|compiler| {
            let mut parsing_context = compiler.parse();
            parsing_context.add_file(
                compiler
                    .sess()
                    .source_map()
                    .new_source_file(PathBuf::from("test.sol"), src)
                    .unwrap(),
            );
            // load file
            parsing_context.parse();
            let _ = compiler.lower_asts().unwrap();
        })
    }
}

fn session() -> Session {
    Session::builder()
        .with_stderr_emitter_and_color(solar::parse::interface::ColorChoice::Always)
        .single_threaded()
        .build()
}

pub struct Solang;
impl Parser for Solang {
    fn name(&self) -> &'static str {
        "solang"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::lex_and_parse()
    }

    fn lex(&self, src: &str, _: &mut dyn Any) {
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

    fn parse(&self, src: &str, _: &mut dyn Any) {
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

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, src: &str, _: &mut dyn Any) {
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

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, src: &str, _: &mut dyn Any) {
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
