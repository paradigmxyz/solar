use super::{AnalysisResultAccumulator, GlobalState, SymbolTables, analyze, snapshot};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use std::sync::Arc;

mod did;
mod import_edits;
mod will;

fn analyze_project(project: &TestProject) -> SymbolTables {
    let mut results = AnalysisResultAccumulator::default();
    for batch in snapshot(project).analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            results.push(analyze(batch));
        }
    }
    results.finish().symbol_tables
}

fn state(project: &TestProject) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    *state.symbol_tables.write() = analyze_project(project);
    state
}
