use std::{collections::HashMap, ops::ControlFlow, sync::Arc};

use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use lsp_types::{
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    PublishDiagnosticsParams, ServerInfo,
};
use solar_config::version::SHORT_VERSION;
use solar_interface::{
    Session,
    data_structures::sync::RwLock,
    diagnostics::{DiagCtxt, InMemoryEmitter},
    source_map::FileName,
};
use solar_sema::Compiler;
use tokio::task::JoinHandle;

use crate::{NotifyResult, config::negotiate_capabilities, proto, vfs::Vfs};

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    analysis_version: usize,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self { client, vfs: Arc::new(Default::default()), analysis_version: 0 }
    }

    pub(crate) fn on_initialize(
        &mut self,
        params: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        let (capabilities, _config) = negotiate_capabilities(params);

        std::future::ready(Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
        }))
    }

    pub(crate) fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
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
        self.analysis_version += 1;
        let version = self.analysis_version;
        self.spawn_with_snapshot(move |mut snapshot| {
            // todo: if this errors, we should notify the user
            // todo: set base path to project root
            // todo: remappings
            let (emitter, diag_buffer) = InMemoryEmitter::new();
            let sess = Session::builder().dcx(DiagCtxt::new(Box::new(emitter))).build();

            let mut compiler = Compiler::new(sess);
            let _ = compiler.enter_mut(move |compiler| -> solar_interface::Result<_> {
                // Parse the files.
                let mut parsing_context = compiler.parse();
                // todo: unwraps
                parsing_context.add_files(snapshot.vfs.read().iter().map(|(path, contents)| {
                    compiler
                        .sess()
                        .source_map()
                        .new_source_file(
                            FileName::real(path.as_path().unwrap()),
                            contents.to_string(),
                        )
                        .unwrap()
                }));

                parsing_context.parse();

                // Perform lowering and analysis.
                // We should never encounter `ControlFlow::Break` because we do not stop after
                // parsing, so we ignore the return.
                // todo: handle errors (currently this always errors?)
                let _ = compiler.lower_asts();
                let _ = compiler.analysis();

                // todo handle diagnostic clearing
                // todo clean this mess up boya
                let diagnostics: HashMap<_, Vec<_>> = diag_buffer
                    .read()
                    .iter()
                    .filter_map(|diag| proto::diagnostic(compiler.sess().source_map(), diag))
                    .fold(HashMap::new(), |mut diags, (path, diag)| {
                        diags.entry(path).or_default().push(diag);
                        diags
                    });
                for (uri, diagnostics) in diagnostics.into_iter() {
                    let _ = snapshot.client.publish_diagnostics(PublishDiagnosticsParams::new(
                        uri,
                        diagnostics,
                        None,
                    ));
                }

                Ok(())
            });
        });
    }

    fn snapshot(&self) -> GlobalStateSnapshot {
        GlobalStateSnapshot { client: self.client.clone(), vfs: self.vfs.clone() }
    }

    fn spawn_with_snapshot<T: Send + 'static>(
        &self,
        f: impl FnOnce(GlobalStateSnapshot) -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let snapshot = self.snapshot();
        tokio::task::spawn_blocking(move || f(snapshot))
    }
}

pub(crate) struct GlobalStateSnapshot {
    client: ClientSocket,
    vfs: Arc<RwLock<Vfs>>,
}
