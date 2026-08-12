use super::{
    AnalysisOutput, AnalysisOutputAccumulator, GlobalState, SymbolTables, analyze_cancellable,
    snapshot,
};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use std::sync::Arc;

mod did;
mod import_edits;
mod will;

fn analyze_project_output(project: &TestProject) -> AnalysisOutput {
    let mut outputs = AnalysisOutputAccumulator::default();
    for batch in snapshot(project).analysis_batches(Vec::new()) {
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
    analyze_project_output(project).result.symbol_tables
}

fn state(project: &TestProject) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    let output = analyze_project_output(project);
    *state.symbol_tables.write() = output.result.symbol_tables;
    state.analysis_commit.lock().analysis_paths = output.analysis_paths;
    state
}
