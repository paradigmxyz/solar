use super::{
    AnalysisOutput, AnalysisOutputAccumulator, GlobalState, SymbolTables, analyze_cancellable,
    snapshot_with_config,
};
use crate::{config::Config, test_support::TestProject};
use async_lsp::ClientSocket;
use lsp_types::{FileRename, RenameFilesParams, Url};
use std::{path::Path, sync::Arc};

mod did;
mod import_edits;
mod will;

fn analyze_project_output(project: &TestProject, config: Config) -> AnalysisOutput {
    let mut outputs = AnalysisOutputAccumulator::default();
    for batch in snapshot_with_config(config, project.vfs()).analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            outputs.push(
                analyze_cancellable(batch, &Default::default())
                    .expect("fresh analysis cancellation cannot be cancelled"),
            );
        }
    }
    outputs.finish()
}

fn analyze_project(project: &TestProject) -> SymbolTables {
    analyze_project_output(project, project.config()).result.symbol_tables
}

fn state(project: &TestProject) -> GlobalState {
    state_with_config(project, project.config())
}

fn state_with_config(project: &TestProject, config: Config) -> GlobalState {
    let output = analyze_project_output(project, config.clone());
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    *state.vfs.write() = project.vfs();
    state.symbol_tables.store(Arc::new(output.result.symbol_tables));
    state.analysis_commit.lock().analysis_paths = output.analysis_paths;
    state
}

fn uri(path: impl AsRef<Path>) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

fn rename_params(
    moves: impl IntoIterator<Item = (impl AsRef<Path>, impl AsRef<Path>)>,
) -> RenameFilesParams {
    let files = moves
        .into_iter()
        .map(|(old, new)| FileRename { old_uri: uri(old), new_uri: uri(new) })
        .collect();
    RenameFilesParams { files }
}
