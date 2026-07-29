#![allow(clippy::disallowed_methods)]

use alloy_primitives::Bytes;
use solar::{
    codegen::{self, Backend, EvmCodegen},
    data_structures::map::FxHashMap,
    parse::interface::{Result, Session},
    sema::CompilerRef,
};

pub use solar::sema::Compiler as SemaCompiler;
use std::{
    any::Any,
    hint::black_box,
    io::Write,
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::Stdio,
};

#[allow(unexpected_cfgs)]
pub const COMPILERS: &[&dyn Compiler] = if cfg!(codspeed) {
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
        let mut sources = vec![
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
            include_source("../testdata/solidity/test/benchmarks/chains.sol", Capabilities::all()),
            // Pre-0.8 source semantics: rejected by 0.8 type rules (unary `-` on
            // unsigned, one-step sign+width conversions).
            include_source("../testdata/UniswapV3.sol", Capabilities::no_codegen()),
            include_source("../testdata/Solarray.sol", Capabilities::all()),
            include_source("../testdata/console.sol", Capabilities::all()),
            include_source("../testdata/Vm.sol", Capabilities::all()),
            include_source("../testdata/safeconsole.sol", Capabilities::all()),
            include_source("../testdata/Seaport.sol", Capabilities::all()),
            include_source("../testdata/Solady.sol", Capabilities::all()),
            // Multi-file concatenation: top-level redeclarations fail symbol
            // resolution in `lower_asts`, so parsing is this source's ceiling.
            include_source("../testdata/Optimism.sol", Capabilities::lex_and_parse()),
        ];
        extend_repro_sources(&mut sources);
        sources
    })
}

pub fn get_src(name: &str) -> &'static Source {
    get_srcs().iter().find(|s| s.name == name).unwrap()
}

fn extend_repro_sources(sources: &mut Vec<Source>) {
    const PATTERNS: &[&str] = &[
        "many_symbols",
        "many_functions",
        "deep_nesting",
        "many_types",
        "large_literals",
        "many_storage",
        "many_events",
        "complex_inheritance",
        "many_mappings",
        "many_modifiers",
    ];
    const SIZES: &[&str] = &[
        // TODO: too many benches
        "small",
        // "medium",
        // "large",
    ];

    for &pattern in PATTERNS {
        for &size in SIZES {
            let rel = format!("../testdata/repros/{pattern}_{size}.sol");
            sources.push(include_source(&rel, Capabilities::all()));
        }
    }
}

fn parse_source(compiler: &mut CompilerRef<'_>, source: &Source) -> Result {
    let mut pcx = compiler.parse();
    let file = compiler
        .sess()
        .source_map()
        .new_source_file(PathBuf::from(source.path), source.src)
        .unwrap();
    pcx.add_file(file);
    pcx.parse();
    compiler.dcx().has_errors()
}

fn codegen_source(compiler: &mut CompilerRef<'_>, source: &Source) -> Result {
    parse_source(compiler, source)?;
    let ControlFlow::Continue(()) = compiler.lower_asts()? else { return Ok(()) };
    let ControlFlow::Continue(()) = compiler.analysis()? else { return Ok(()) };
    codegen_contracts(compiler)
}

fn codegen_contracts(compiler: &mut CompilerRef<'_>) -> Result {
    let gcx = compiler.gcx();
    let mut bytecodes = FxHashMap::default();
    for contract_id in gcx.hir.contract_ids() {
        if !gcx.hir.contract(contract_id).can_be_deployed() {
            continue;
        }
        ensure_contract_bytecode(gcx, contract_id, &mut bytecodes)?;
    }
    Ok(())
}

/// Generates a contract's deployment bytecode, recursing into its `new`
/// dependencies first so creation-bytecode references resolve.
fn ensure_contract_bytecode(
    gcx: solar::sema::Gcx<'_>,
    contract_id: solar::sema::hir::ContractId,
    bytecodes: &mut FxHashMap<solar::sema::hir::ContractId, Bytes>,
) -> Result {
    if bytecodes.contains_key(&contract_id) {
        return Ok(());
    }
    // Valid code cannot have recursive creation dependencies; seed the entry
    // so an unexpected cycle terminates instead of recursing forever.
    bytecodes.insert(contract_id, Bytes::new());
    for dep in codegen::lower::contract_bytecode_dependencies(gcx, contract_id).iter() {
        ensure_contract_bytecode(gcx, dep, bytecodes)?;
    }
    let mut module = codegen::lower::lower_contract_with_bytecodes(gcx, contract_id, bytecodes);
    gcx.dcx().has_errors()?;
    let artifact = EvmCodegen::new(gcx).lower_module(&mut module);
    bytecodes.insert(contract_id, artifact.deployment.clone());
    black_box(artifact);
    Ok(())
}

fn parse_project(compiler: &mut CompilerRef<'_>, project: &ProjectSource) -> Result {
    let mut pcx = compiler.parse();
    pcx.par_load_files_with_contents(
        project
            .files
            .iter()
            .map(|&(name, content)| (PathBuf::from(name), content))
            .collect::<Vec<_>>(),
    )?;
    pcx.parse();
    compiler.dcx().has_errors()
}

fn codegen_project(compiler: &mut CompilerRef<'_>, project: &ProjectSource) -> Result {
    parse_project(compiler, project)?;
    let ControlFlow::Continue(()) = compiler.lower_asts()? else { return Ok(()) };
    let ControlFlow::Continue(()) = compiler.analysis()? else { return Ok(()) };
    codegen_contracts(compiler)
}

/// Builds a session configured for a project source: its import remappings
/// applied, single-threaded, with codegen enabled.
fn project_session(project: &ProjectSource) -> Session {
    let mut opts = solar::config::CompileOpts {
        threads: solar::config::Threads::resolve(1),
        unstable: solar::config::UnstableOpts { codegen: true, ..Default::default() },
        ..Default::default()
    };
    opts.import_remappings =
        project.remappings.iter().map(|remapping| remapping.parse().unwrap()).collect();
    Session::builder()
        .with_stderr_emitter_and_color(solar::parse::interface::ColorChoice::Always)
        .opts(opts)
        .build()
}

/// Creates the per-iteration compiler state for a project bench.
pub fn project_setup(project: &ProjectSource) -> SemaCompiler {
    SemaCompiler::new(project_session(project))
}

pub fn run_project_parse(compiler: &mut SemaCompiler, project: &ProjectSource) {
    compiler.enter_mut(|compiler| parse_project(compiler, project).unwrap());
}

pub fn run_project_lower(compiler: &mut SemaCompiler, project: &ProjectSource) {
    compiler.enter_mut(|compiler| {
        parse_project(compiler, project).unwrap();
        let _ = compiler.lower_asts().unwrap();
    });
}

pub fn run_project_codegen(compiler: &mut SemaCompiler, project: &ProjectSource) {
    compiler.enter_mut(|compiler| codegen_project(compiler, project).unwrap());
}

/// `include!` at runtime, since the submodule may not be initialized.
fn include_source(path: &str, capabilities: Capabilities) -> Source {
    let source = match std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)) {
        Ok(source) => source,
        Err(e) => panic!(
            "failed to read {path}: {e};\n\
             you may need to initialize submodules: `git submodule update --init --checkout`"
        ),
    };
    source_from_path(path, source.leak(), capabilities)
}

fn source_from_path(path: &str, src: &'static str, capabilities: Capabilities) -> Source {
    let path = Path::new(path).canonicalize().unwrap().to_string_lossy().into_owned().leak();
    Source { name: Path::new(path).file_stem().unwrap().to_str().unwrap(), path, src, capabilities }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: &'static str,
    pub path: &'static str,
    pub src: &'static str,
    pub capabilities: Capabilities,
}

/// A whole-project compilation input: every source `forge build` compiles,
/// test suite included, extracted as solc Standard JSON.
///
/// Single flattened contracts undersell the workload that makes compilers
/// slow in practice; a project's build input is typically half test and mock
/// code by file count and several times the flattened core by volume.
#[derive(Clone, Debug)]
pub struct ProjectSource {
    pub name: &'static str,
    /// `(source unit name, content)` pairs.
    pub files: Vec<(&'static str, &'static str)>,
    /// Import remappings from the project's build configuration.
    pub remappings: Vec<&'static str>,
    /// Total source bytes across every file.
    pub bytes: u64,
    pub capabilities: Capabilities,
}

pub fn get_projects() -> &'static [ProjectSource] {
    static CACHE: std::sync::OnceLock<Vec<ProjectSource>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        // Whole-project inputs mirroring solc's external benchmarks
        // (`test/benchmarks/external-setup.sh` upstream): pinned Foundry
        // projects compiled with their full test suites.
        //
        // All nine projects compile clean through codegen.
        vec![
            include_project("../testdata/projects/seaport-1.6.json", Capabilities::all()),
            include_project("../testdata/projects/openzeppelin-5.6.1.json", Capabilities::all()),
            include_project("../testdata/projects/solady-0.1.26.json", Capabilities::all()),
            include_project("../testdata/projects/v4-core-4.0.0.json", Capabilities::all()),
            include_project("../testdata/projects/morpho-blue-1.0.0.json", Capabilities::all()),
            include_project("../testdata/projects/forge-std-1.16.1.json", Capabilities::all()),
            include_project("../testdata/projects/prb-math-4.1.1.json", Capabilities::all()),
            include_project("../testdata/projects/solmate-6.json", Capabilities::all()),
            include_project("../testdata/projects/solarray-a547630.json", Capabilities::all()),
        ]
    })
}

fn include_project(path: &str, capabilities: Capabilities) -> ProjectSource {
    let full_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    let text = match std::fs::read_to_string(&full_path) {
        Ok(text) => text,
        Err(e) => panic!("failed to read {path}: {e}"),
    };
    let input: serde_json::Value = match serde_json::from_str(&text) {
        Ok(input) => input,
        Err(e) => panic!("failed to parse {path}: {e}"),
    };
    let sources = input["sources"].as_object().unwrap_or_else(|| panic!("{path}: no sources"));
    let mut files = Vec::with_capacity(sources.len());
    let mut bytes = 0;
    for (name, source) in sources {
        let content = source["content"]
            .as_str()
            .unwrap_or_else(|| panic!("{path}: source `{name}` has no content"));
        bytes += content.len() as u64;
        files.push((
            name.clone().leak() as &'static str,
            content.to_string().leak() as &'static str,
        ));
    }
    let remappings = input["settings"]["remappings"]
        .as_array()
        .map(|remappings| {
            remappings
                .iter()
                .map(|remapping| remapping.as_str().unwrap().to_string().leak() as &'static str)
                .collect()
        })
        .unwrap_or_default();
    let name = Path::new(path).file_stem().unwrap().to_str().unwrap().to_string().leak();
    ProjectSource { name, files, remappings, bytes, capabilities }
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    lex: bool,
    lower: bool,
    codegen: bool,
}

impl Capabilities {
    pub fn all() -> Self {
        Self { lex: true, lower: true, codegen: true }
    }

    pub fn parse_only() -> Self {
        Self { lex: false, lower: false, codegen: false }
    }

    pub fn lex_and_parse() -> Self {
        Self { lex: true, lower: false, codegen: false }
    }

    pub fn no_codegen() -> Self {
        Self { lex: true, lower: true, codegen: false }
    }

    pub fn can_lex(&self) -> bool {
        self.lex
    }

    pub fn can_lower(&self) -> bool {
        self.lower
    }

    pub fn can_codegen(&self) -> bool {
        self.codegen
    }
}

pub trait Compiler {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn setup(&self, _source: &Source) -> Box<dyn Any> {
        Box::new(())
    }
    fn lex(&self, _source: &Source, _setup: &mut dyn Any) {}
    fn parse(&self, source: &Source, setup: &mut dyn Any);
    fn lower(&self, _source: &Source, _setup: &mut dyn Any) {}
    fn codegen(&self, _source: &Source, _setup: &mut dyn Any) {}
}

pub struct Solc;
impl Compiler for Solc {
    fn name(&self) -> &'static str {
        "solc"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, source: &Source, _: &mut dyn Any) {
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
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(source.src.as_bytes())
            .expect("failed to write to stdin");
        let output = child.wait_with_output().expect("failed to wait for child");
        if !output.status.success() {
            panic!("solc failed.\ncmd: {cmd:?}\nout: {output:#?}");
        }
        let _stdout = String::from_utf8(output.stdout).expect("failed to read stdout");
    }
}

pub struct Solar;
impl Compiler for Solar {
    fn name(&self) -> &'static str {
        "solar"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    fn setup(&self, _source: &Source) -> Box<dyn Any> {
        Box::new(SemaCompiler::new(session()))
    }

    fn lex(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_ref::<SemaCompiler>().unwrap();
        compiler.enter(|compiler| {
            for token in solar::parse::Lexer::new(compiler.sess(), source.src) {
                black_box(token);
            }
            compiler.dcx().has_errors().unwrap();
        });
    }

    fn parse(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<SemaCompiler>().unwrap();
        compiler
            .enter_mut(|compiler| -> solar::parse::interface::Result {
                let arena = solar::parse::ast::Arena::new();
                let filename = PathBuf::from(source.path);
                let mut parser = solar::parse::Parser::from_source_code(
                    compiler.sess(),
                    &arena,
                    filename.into(),
                    source.src,
                )?;
                let result = parser.parse_file().map_err(|e| e.emit())?;
                compiler.dcx().has_errors()?;
                black_box(result);
                Ok(())
            })
            .unwrap();
    }

    fn lower(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<SemaCompiler>().unwrap();
        compiler.enter_mut(|compiler| {
            parse_source(compiler, source).unwrap();
            let _ = compiler.lower_asts().unwrap();
        })
    }

    fn codegen(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<SemaCompiler>().unwrap();
        compiler.enter_mut(|compiler| codegen_source(compiler, source).unwrap())
    }
}

fn session() -> Session {
    Session::builder()
        .with_stderr_emitter_and_color(solar::parse::interface::ColorChoice::Always)
        .opts(solar::config::CompileOpts {
            threads: solar::config::Threads::resolve(1),
            unstable: solar::config::UnstableOpts { codegen: true, ..Default::default() },
            ..Default::default()
        })
        .build()
}

pub struct Solang;
impl Compiler for Solang {
    fn name(&self) -> &'static str {
        "solang"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::lex_and_parse()
    }

    fn lex(&self, source: &Source, _: &mut dyn Any) {
        let mut comments = vec![];
        let mut errors = vec![];
        for token in solang_parser::lexer::Lexer::new(source.src, 0, &mut comments, &mut errors) {
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

    fn parse(&self, source: &Source, _: &mut dyn Any) {
        match solang_parser::parse(source.src, 0) {
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
impl Compiler for Slang {
    fn name(&self) -> &'static str {
        "slang"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, source: &Source, _: &mut dyn Any) {
        let version = semver::Version::new(0, 8, 22);
        let parser = slang_solidity::parser::Parser::create(version).unwrap();
        let rule = slang_solidity::cst::NonterminalKind::SourceUnit;
        let output = parser.parse(rule, source.src);

        let errors = output.errors();
        if !errors.is_empty() {
            for err in errors {
                let range = err.text_range();
                let slice =
                    source.src.get(range.start.utf8..range.end.utf8).unwrap_or("<invalid range>");
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
impl Compiler for TreeSitter {
    fn name(&self) -> &'static str {
        "tree-sitter"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::parse_only()
    }

    fn parse(&self, source: &Source, _: &mut dyn Any) {
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
        let tree = parser.parse(source.src, None).unwrap();
        if tree.root_node().has_error() {
            on_error(source.src, &tree);
        }
    }
}
