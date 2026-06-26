use std::{
    collections::{HashMap, HashSet},
    ops::ControlFlow,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use lsp_types::{
    Diagnostic, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    PublishDiagnosticsParams, Registration, RegistrationParams, ServerInfo, Url, WatchKind,
    notification::{DidChangeWatchedFiles, Notification},
};
use solar_config::version::SHORT_VERSION;
use solar_interface::{
    Session,
    data_structures::sync::RwLock,
    diagnostics::{DiagCtxt, InMemoryEmitter},
    source_map::{FileName, SourceMap},
};
use solar_sema::Compiler;
use tokio::task::JoinHandle;

use crate::{
    NotifyResult,
    config::{Config, negotiate_capabilities},
    proto,
    symbols::SymbolTables,
    vfs::Vfs,
};

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    pub(crate) symbol_tables: Arc<RwLock<SymbolTables>>,
    published_diagnostic_uris: Arc<RwLock<HashSet<Url>>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self {
            client,
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(0)),
            symbol_tables: Arc::new(Default::default()),
            published_diagnostic_uris: Arc::new(Default::default()),
            config: Arc::new(Default::default()),
        }
    }

    pub(crate) fn on_initialize(
        &mut self,
        params: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        let (capabilities, mut config) = negotiate_capabilities(params);

        config.rediscover_workspaces();

        self.config = Arc::new(config);
        std::future::ready(Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
        }))
    }

    pub(crate) fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
        if self.config.supports_watched_file_dynamic_registration() {
            let mut client = self.client.clone();
            tokio::spawn(async move {
                if let Err(error) =
                    client.register_capability(watched_file_registration_params()).await
                {
                    tracing::warn!(%error, "failed to register watched-file notifications");
                }
            });
        }

        let _ = self.client.log_message(LogMessageParams {
            typ: MessageType::INFO,
            message: "solar initialized".into(),
        });
        ControlFlow::Continue(())
    }

    /// Parses, lowers, and performs analysis on project files, including in-memory only files.
    ///
    /// Each time analysis is triggered, a version is assigned to the analysis. A snapshot is then
    /// taken of the global state ([`GlobalStateSnapshot`]) and analysis is performed on
    /// the entire project in a separate thread.
    ///
    /// Currently, Solar is sufficiently fast at parsing and lowering even large Solidity projects,
    /// so while analysing the entire project is relatively expensive compared to incremental
    /// analysis, it is still fast enough for most workloads. A potential improvement would be to
    /// enable incremental parsing and analysis in Solar using e.g. [`salsa`].
    ///
    /// [`salsa`]: https://docs.rs/salsa/latest/salsa/
    pub(crate) fn recompute(&mut self) {
        self.recompute_with_disk_files(Vec::new());
    }

    pub(crate) fn recompute_with_disk_files(&mut self, disk_paths: Vec<PathBuf>) {
        let version = self.analysis_version.fetch_add(1, Ordering::AcqRel) + 1;
        self.spawn_with_snapshot(move |mut snapshot| {
            let (files, file_uris) = snapshot.analysis_inputs(disk_paths);
            if files.is_empty() {
                if snapshot.is_current(version) {
                    snapshot.set_symbol_tables(SymbolTables::default());
                    snapshot.publish_diagnostic_set(file_uris, HashMap::new());
                }
                return;
            }

            let result = analyze(files);
            if snapshot.is_current(version) {
                snapshot.set_symbol_tables(result.symbol_tables);
                snapshot.publish_diagnostic_set(file_uris, result.diagnostics);
            }
        });
    }

    fn snapshot(&self) -> GlobalStateSnapshot {
        GlobalStateSnapshot {
            client: self.client.clone(),
            vfs: self.vfs.clone(),
            analysis_version: self.analysis_version.clone(),
            symbol_tables: self.symbol_tables.clone(),
            published_diagnostic_uris: self.published_diagnostic_uris.clone(),
        }
    }

    fn spawn_with_snapshot<T: Send + 'static>(
        &self,
        f: impl FnOnce(GlobalStateSnapshot) -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let snapshot = self.snapshot();
        tokio::task::spawn_blocking(move || f(snapshot))
    }
}

struct AnalysisResult {
    diagnostics: HashMap<Url, Vec<Diagnostic>>,
    symbol_tables: SymbolTables,
}

fn analyze(files: Vec<(PathBuf, String)>) -> AnalysisResult {
    if files.is_empty() {
        return AnalysisResult {
            diagnostics: HashMap::new(),
            symbol_tables: SymbolTables::default(),
        };
    }

    // todo: if this errors, we should notify the user
    // todo: set base path to project root
    // todo: remappings
    let (emitter, diag_buffer) = InMemoryEmitter::new();
    let sess = Session::builder().dcx(DiagCtxt::new(Box::new(emitter))).build();

    let mut compiler = Compiler::new(sess);
    let _ = compiler.enter_mut(move |compiler| -> solar_interface::Result<_> {
        let mut parsing_context = compiler.parse();
        // todo: unwraps
        parsing_context.add_files(files.into_iter().map(|(path, contents)| {
            compiler.sess().source_map().new_source_file(FileName::real(path), contents).unwrap()
        }));

        parsing_context.parse();

        // We should never encounter `ControlFlow::Break` because we do not stop after parsing,
        // so we ignore the return.
        // todo: handle errors (currently this always errors?)
        let _ = compiler.lower_asts();
        let _ = compiler.analysis();

        Ok(())
    });

    let symbol_tables = compiler.enter(|compiler| SymbolTables::build(compiler.gcx()));
    let diagnostics = compiler.enter(|compiler| {
        diag_buffer
            .read()
            .iter()
            .filter_map(|diag| proto::diagnostic(compiler.sess().source_map(), diag))
            .fold(HashMap::<Url, Vec<Diagnostic>>::new(), |mut diags, (path, diag)| {
                diags.entry(path).or_default().push(diag);
                diags
            })
    });

    AnalysisResult { diagnostics, symbol_tables }
}

fn watched_file_registration_params() -> RegistrationParams {
    let kind = Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete);
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![
            FileSystemWatcher { glob_pattern: GlobPattern::String("**/*.sol".into()), kind },
            FileSystemWatcher { glob_pattern: GlobPattern::String("**/foundry.toml".into()), kind },
        ],
    };

    RegistrationParams {
        registrations: vec![Registration {
            id: "solar-watched-files".into(),
            method: DidChangeWatchedFiles::METHOD.into(),
            register_options: Some(serde_json::to_value(options).unwrap()),
        }],
    }
}

pub(crate) struct GlobalStateSnapshot {
    client: ClientSocket,
    vfs: Arc<RwLock<Vfs>>,
    analysis_version: Arc<AtomicUsize>,
    symbol_tables: Arc<RwLock<SymbolTables>>,
    published_diagnostic_uris: Arc<RwLock<HashSet<Url>>>,
}

impl GlobalStateSnapshot {
    fn is_current(&self, version: usize) -> bool {
        self.analysis_version.load(Ordering::Acquire) == version
    }

    fn analysis_inputs(&self, disk_paths: Vec<PathBuf>) -> (Vec<(PathBuf, String)>, Vec<Url>) {
        let mut files = self
            .vfs
            .read()
            .iter()
            .filter_map(|(path, contents)| {
                Some((path.as_path()?.to_path_buf(), contents.to_string()))
            })
            .collect::<Vec<_>>();
        let mut file_uris =
            files.iter().filter_map(|(path, _)| Url::from_file_path(path).ok()).collect::<Vec<_>>();
        let mut seen_paths = files.iter().map(|(path, _)| path.clone()).collect::<HashSet<_>>();
        let source_map = SourceMap::empty();

        for path in disk_paths {
            if let Ok(uri) = Url::from_file_path(&path) {
                file_uris.push(uri);
            }

            if !seen_paths.insert(path.clone()) {
                continue;
            }

            if let Ok(contents) = source_map.file_loader().load_file(&path) {
                files.push((path, contents));
            }
        }

        (files, file_uris)
    }

    fn set_symbol_tables(&mut self, symbol_tables: SymbolTables) {
        *self.symbol_tables.write() = symbol_tables;
    }

    fn publish_diagnostic_set(
        &mut self,
        file_uris: Vec<Url>,
        mut diagnostics: HashMap<Url, Vec<Diagnostic>>,
    ) {
        let mut uris = file_uris.into_iter().collect::<HashSet<_>>();
        uris.extend(diagnostics.keys().cloned());

        let mut published_diagnostic_uris = self.published_diagnostic_uris.write();
        for uri in uris {
            let uri_diagnostics = diagnostics.remove(&uri).unwrap_or_default();
            if !uri_diagnostics.is_empty() {
                published_diagnostic_uris.insert(uri.clone());
            } else {
                published_diagnostic_uris.remove(&uri);
            }
            let _ = self.client.publish_diagnostics(PublishDiagnosticsParams::new(
                uri,
                uri_diagnostics,
                None,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use lsp_types::{Diagnostic, Position, Range, WatchKind, notification::Notification};

    use crate::symbols::DeclarationKind;

    use super::*;

    #[test]
    fn watched_file_registration_watches_solidity_and_foundry_manifests() {
        let [registration] = watched_file_registration_params().registrations.try_into().unwrap();
        assert_eq!(registration.id, "solar-watched-files");
        assert_eq!(registration.method, lsp_types::notification::DidChangeWatchedFiles::METHOD);

        assert_eq!(
            registration.register_options,
            Some(serde_json::json!({
                "watchers": [
                    { "globPattern": "**/*.sol", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                    { "globPattern": "**/foundry.toml", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                ],
            }))
        );
    }

    #[test]
    fn publish_diagnostic_set_retains_unrelated_diagnostics() {
        let clean_uri = Url::parse("file:///workspace/src/Clean.sol").unwrap();
        let diagnostic_uri = Url::parse("file:///workspace/src/Error.sol").unwrap();
        let previous_uri = Url::parse("file:///workspace/src/Previous.sol").unwrap();
        let published_diagnostic_uris =
            Arc::new(RwLock::new(HashSet::from([clean_uri.clone(), previous_uri.clone()])));
        let mut snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            symbol_tables: Arc::new(Default::default()),
            published_diagnostic_uris: published_diagnostic_uris.clone(),
        };

        let diagnostic = Diagnostic::new_simple(
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            "error".into(),
        );
        snapshot.publish_diagnostic_set(
            vec![clean_uri.clone()],
            HashMap::from([(diagnostic_uri.clone(), vec![diagnostic])]),
        );

        let published = published_diagnostic_uris.read();
        assert!(!published.contains(&clean_uri));
        assert!(published.contains(&diagnostic_uri));
        assert!(published.contains(&previous_uri));
    }

    #[test]
    fn analysis_inputs_reads_disk_files() {
        let path = std::env::temp_dir()
            .join(format!("solar-lsp-analysis-inputs-{}-Saved.sol", std::process::id()));
        std::fs::write(&path, "contract C { function f() public { number+; } }").unwrap();
        let uri = Url::from_file_path(&path).unwrap();
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            symbol_tables: Arc::new(Default::default()),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let (files, uris) = snapshot.analysis_inputs(vec![path.clone()]);

        std::fs::remove_file(&path).unwrap();
        assert_eq!(files, vec![(path, "contract C { function f() public { number+; } }".into())]);
        assert_eq!(uris, vec![uri]);
    }

    #[test]
    fn analysis_inputs_keeps_unreadable_disk_uri() {
        let path = std::env::temp_dir()
            .join(format!("solar-lsp-analysis-inputs-{}-Missing.sol", std::process::id()));
        let uri = Url::from_file_path(&path).unwrap();
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            symbol_tables: Arc::new(Default::default()),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let (files, uris) = snapshot.analysis_inputs(vec![path]);

        assert!(files.is_empty());
        assert_eq!(uris, vec![uri]);
    }

    #[test]
    fn analyze_builds_declaration_symbol_table() {
        let path = std::env::temp_dir()
            .join(format!("solar-lsp-symbols-{}-Symbols.sol", std::process::id()));
        let uri = Url::from_file_path(&path).unwrap();
        let result = analyze(vec![(
            path,
            r#"
contract C {
    uint256 public x;
    struct S { uint256 field; }
    constructor() {}
    fallback() external {}
    receive() external payable {}
    function f(uint256 y) public returns (uint256 z) {
        uint256 local = x + y;
        return local;
    }
}
enum E { A }
"#
            .into(),
        )]);

        assert!(result.diagnostics.is_empty());

        let declarations = result.symbol_tables.file_declarations(&uri).collect::<Vec<_>>();
        assert_declaration(&declarations, "C", DeclarationKind::Contract);
        assert_declaration(&declarations, "x", DeclarationKind::Variable);
        assert_declaration(&declarations, "S", DeclarationKind::Struct);
        assert_declaration(&declarations, "field", DeclarationKind::Variable);
        assert_declaration(&declarations, "constructor", DeclarationKind::Function);
        assert_declaration(&declarations, "fallback", DeclarationKind::Function);
        assert_declaration(&declarations, "receive", DeclarationKind::Function);
        assert_declaration(&declarations, "f", DeclarationKind::Function);
        assert_declaration(&declarations, "y", DeclarationKind::Variable);
        assert_declaration(&declarations, "z", DeclarationKind::Variable);
        assert_declaration(&declarations, "local", DeclarationKind::Variable);
        assert_declaration(&declarations, "E", DeclarationKind::Enum);
        assert_declaration(&declarations, "A", DeclarationKind::EnumVariant);

        assert_parent(&declarations, "x", "C");
        assert_parent(&declarations, "field", "S");
        assert_parent(&declarations, "constructor", "C");
        assert_parent(&declarations, "y", "f");
        assert_parent(&declarations, "z", "f");
        assert_parent(&declarations, "local", "f");
        assert_parent(&declarations, "A", "E");

        assert_eq!(
            declarations
                .iter()
                .filter(|symbol| symbol.name == "x" && symbol.kind == DeclarationKind::Variable)
                .count(),
            1
        );
        assert_eq!(declarations.len(), result.symbol_tables.declarations().len());
    }

    fn assert_parent(
        declarations: &[&crate::symbols::DeclarationSymbol],
        name: &str,
        parent: &str,
    ) {
        let declaration = find_declaration(declarations, name);
        let parent_id = declaration.parent.unwrap_or_else(|| {
            panic!("declaration `{name}` has no parent in {declarations:#?}");
        });
        let parent_declaration = declarations
            .iter()
            .find(|candidate| candidate.id == parent_id)
            .unwrap_or_else(|| panic!("parent {parent_id:?} for `{name}` not found"));
        assert_eq!(parent_declaration.name, parent);
    }

    fn assert_declaration(
        declarations: &[&crate::symbols::DeclarationSymbol],
        name: &str,
        kind: DeclarationKind,
    ) {
        assert!(
            declarations.iter().any(|symbol| symbol.name == name && symbol.kind == kind),
            "missing {kind:?} declaration `{name}` in {declarations:#?}"
        );
    }

    fn find_declaration<'a>(
        declarations: &'a [&crate::symbols::DeclarationSymbol],
        name: &str,
    ) -> &'a crate::symbols::DeclarationSymbol {
        declarations
            .iter()
            .copied()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing declaration `{name}` in {declarations:#?}"))
    }
}
