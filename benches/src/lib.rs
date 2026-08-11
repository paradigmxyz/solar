#![allow(clippy::disallowed_methods)]

use alloy_primitives::Bytes;
use flate2::read::GzDecoder;
use solar::{
    codegen::{self, Backend, EvmCodegen},
    data_structures::map::FxHashMap,
    parse::interface::{Result, Session},
    sema::{Compiler as SemaCompiler, CompilerRef},
};
use std::{
    any::Any,
    borrow::Cow,
    hint::black_box,
    io::Write,
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::Stdio,
};

#[allow(unexpected_cfgs)]
pub const IS_CODSPEED: bool = cfg!(codspeed);

pub const COMPILERS: &[&dyn Compiler] = if IS_CODSPEED {
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
        let mut sources = common_sources();
        extend_repro_sources(&mut sources);

        // Whole-project inputs mirroring solc's external benchmarks
        // (`test/benchmarks/external-setup.sh` upstream): pinned Foundry
        // projects compiled with their full test suites.
        //
        // OpenZeppelin, v4-core, and PRBMath currently stop before codegen
        // on unsupported compiler behavior. Only project codegen cases that
        // keep the full simulated suite under ten minutes opt in below.
        sources.extend([
            include_source("../testdata/projects/seaport-1.6.json.gz", Capabilities::all()),
            include_source(
                "../testdata/codegen-runtime/projects/openzeppelin-5.6.1.json.gz",
                Capabilities::no_codegen(),
            ),
            include_source(
                "../testdata/codegen-runtime/projects/solady-0.1.26.json.gz",
                Capabilities::all(),
            ),
            include_source(
                "../testdata/projects/v4-core-4.0.0.json.gz",
                Capabilities::no_codegen(),
            ),
            include_source("../testdata/projects/morpho-blue-1.0.0.json.gz", Capabilities::all())
                .with_codspeed_codegen(),
            include_source("../testdata/projects/forge-std-1.16.1.json.gz", Capabilities::all())
                .with_codspeed_codegen(),
            include_source(
                "../testdata/projects/prb-math-4.1.1.json.gz",
                Capabilities::no_codegen(),
            ),
            include_source("../testdata/projects/solmate-6.json.gz", Capabilities::all())
                .with_codspeed_codegen(),
            include_source("../testdata/projects/solarray-a547630.json.gz", Capabilities::all())
                .with_codspeed_codegen(),
        ]);

        sources
    })
}

pub fn get_src(name: &str) -> &'static Source {
    static COMMON_SOURCES: std::sync::OnceLock<Vec<Source>> = std::sync::OnceLock::new();
    if let Some(source) =
        COMMON_SOURCES.get_or_init(common_sources).iter().find(|source| source.name == name)
    {
        return source;
    }
    get_srcs().iter().find(|s| s.name == name).unwrap()
}

fn common_sources() -> Vec<Source> {
    vec![
        Source {
            name: Cow::Borrowed("empty"),
            files: vec![(Cow::Borrowed(""), Cow::Borrowed(""))],
            remappings: Vec::new(),
            bytes: 0,
            capabilities: Capabilities::all(),
            codspeed_codegen: true,
        },
        include_source("../testdata/Counter.sol", Capabilities::all()),
        include_source("../testdata/solidity/test/benchmarks/verifier.sol", Capabilities::all()),
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
        include_source("../testdata/Arithmetic.sol", Capabilities::all()),
        include_source("../testdata/Factorial.sol", Capabilities::all()),
        include_source("../testdata/SumArray.sol", Capabilities::all()),
    ]
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
    pcx.par_load_files_with_contents(
        source
            .files
            .iter()
            .map(|(name, content)| (PathBuf::from(name.as_ref()), content.to_string()))
            .collect::<Vec<_>>(),
    )?;
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
    for dep in gcx.contract_bytecode_dependencies(contract_id) {
        ensure_contract_bytecode(gcx, dep, bytecodes)?;
    }
    let mut module = codegen::lower::lower_contract_with_bytecodes(gcx, contract_id, bytecodes);
    gcx.dcx().has_errors()?;
    let artifact = EvmCodegen::new(gcx).lower_module(&mut module);
    bytecodes.insert(contract_id, artifact.deployment.clone().into());
    black_box(artifact);
    Ok(())
}

/// `include!` at runtime, since the submodule may not be initialized.
fn include_source(path: &str, capabilities: Capabilities) -> Source {
    let full_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    if !path.ends_with(".json.gz") {
        let content = match std::fs::read_to_string(&full_path) {
            Ok(content) => content,
            Err(e) => panic!(
                "failed to read {path}: {e};\n\
                 you may need to initialize submodules: `git submodule update --init --checkout`"
            ),
        };
        let path = Path::new(path).canonicalize().unwrap().to_string_lossy().into_owned();
        let name = Path::new(&path).file_stem().unwrap().to_str().unwrap().to_owned();
        let bytes = content.len() as u64;
        return Source {
            name: Cow::Owned(name),
            files: vec![(Cow::Owned(path), Cow::Owned(content))],
            remappings: Vec::new(),
            bytes,
            capabilities,
            codspeed_codegen: true,
        };
    }

    let file = match std::fs::File::open(&full_path) {
        Ok(file) => file,
        Err(e) => panic!("failed to read {path}: {e}"),
    };
    let input: serde_json::Value = match serde_json::from_reader(GzDecoder::new(file)) {
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
        files.push((Cow::Owned(name.clone()), Cow::Owned(content.to_owned())));
    }
    let remappings = input["settings"]["remappings"]
        .as_array()
        .map(|remappings| {
            remappings
                .iter()
                .map(|remapping| Cow::Owned(remapping.as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let name = Path::new(path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_suffix(".json.gz")
        .unwrap()
        .to_owned();
    Source {
        name: Cow::Owned(name),
        files,
        remappings,
        bytes,
        capabilities,
        codspeed_codegen: false,
    }
}

/// A single-file or whole-project compilation input.
#[derive(Clone, Debug)]
pub struct Source {
    pub name: Cow<'static, str>,
    /// `(source unit name, content)` pairs.
    pub files: Vec<(Cow<'static, str>, Cow<'static, str>)>,
    /// Import remappings from the build configuration.
    pub remappings: Vec<Cow<'static, str>>,
    /// Total source bytes across every file.
    pub bytes: u64,
    pub capabilities: Capabilities,
    pub codspeed_codegen: bool,
}

impl Source {
    fn with_codspeed_codegen(mut self) -> Self {
        self.codspeed_codegen = true;
        self
    }

    fn single_file(&self) -> (&str, &str) {
        let [(name, content)] = self.files.as_slice() else {
            panic!("`{}` is not a single-file source", self.name)
        };
        (name, content)
    }
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
    fn supports(&self, source: &Source) -> bool {
        source.files.len() == 1
    }
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
        let (_, content) = source.single_file();
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
            .write_all(content.as_bytes())
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

    fn supports(&self, _source: &Source) -> bool {
        true
    }

    fn setup(&self, source: &Source) -> Box<dyn Any> {
        Box::new(SemaCompiler::new(session(source)))
    }

    fn lex(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_ref::<SemaCompiler>().unwrap();
        compiler.enter(|compiler| {
            for (_, content) in &source.files {
                for token in solar::parse::Lexer::new(compiler.sess(), content) {
                    black_box(token);
                }
            }
            compiler.dcx().has_errors().unwrap();
        });
    }

    fn parse(&self, source: &Source, compiler_any: &mut dyn Any) {
        let compiler = compiler_any.downcast_mut::<SemaCompiler>().unwrap();
        compiler.enter_mut(|compiler| parse_source(compiler, source).unwrap());
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

fn session(source: &Source) -> Session {
    let mut opts = solar::config::CompileOpts {
        threads: solar::config::Threads::resolve(1),
        unstable: solar::config::UnstableOpts {
            codegen: true,
            codegen_all_functions: true,
            ..Default::default()
        },
        ..Default::default()
    };
    opts.import_remappings =
        source.remappings.iter().map(|remapping| remapping.parse().unwrap()).collect();
    Session::builder()
        .with_stderr_emitter_and_color(solar::parse::interface::ColorChoice::Always)
        .opts(opts)
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
        let (_, content) = source.single_file();
        let mut comments = vec![];
        let mut errors = vec![];
        for token in solang_parser::lexer::Lexer::new(content, 0, &mut comments, &mut errors) {
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
        let (_, content) = source.single_file();
        match solang_parser::parse(content, 0) {
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
        let (_, content) = source.single_file();
        let version = semver::Version::new(0, 8, 22);
        let parser = slang_solidity::parser::Parser::create(version).unwrap();
        let rule = slang_solidity::cst::NonterminalKind::SourceUnit;
        let output = parser.parse(rule, content);

        let errors = output.errors();
        if !errors.is_empty() {
            for err in errors {
                let range = err.text_range();
                let slice =
                    content.get(range.start.utf8..range.end.utf8).unwrap_or("<invalid range>");
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
        let (_, content) = source.single_file();
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
        let tree = parser.parse(content, None).unwrap();
        if tree.root_node().has_error() {
            on_error(content, &tree);
        }
    }
}
