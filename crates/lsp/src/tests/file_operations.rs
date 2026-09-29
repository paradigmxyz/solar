use super::*;
use lsp_types::{FileRename, RenameFilesParams};

mod did;
mod import_edits;
mod will;

fn analyze_project(project: &TestProject) -> SymbolTables {
    analyze_workspace(&snapshot(project)).result.symbol_tables
}

fn state(project: &TestProject) -> GlobalState {
    state_with_config(project, project.config())
}

fn state_with_config(project: &TestProject, config: Config) -> GlobalState {
    let output = analyze_workspace(&snapshot_with_config(config.clone(), project.vfs()));
    let state = state_with(config);
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
