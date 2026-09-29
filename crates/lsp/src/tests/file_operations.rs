use super::*;
use lsp_types::{FileRename, RenameFilesParams};
use std::fs;

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

/// Rewrites `importer` on disk to `import {new_text};`, applies the `[old, new]` moves of the
/// importer and the target, and checks that the moved import resolves to the moved target.
fn assert_moved_import_resolves(
    project: &TestProject,
    importer: [&Path; 2],
    new_text: &str,
    target: [&Path; 2],
) {
    fs::write(importer[0], format!("import {new_text};\n")).unwrap();
    for [old, new] in [importer, target] {
        if old != new {
            fs::create_dir_all(new.parent().unwrap()).unwrap();
            fs::rename(old, new).unwrap();
        }
    }
    let links = analyze_project(project).document_links(importer[1]);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), target[1]);
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
