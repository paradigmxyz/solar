use lsp_types::{Range, TextEdit, Url};
use solar_config::ImportRemapping;
use solar_interface::data_structures::map::FxHashMap;
use std::{
    collections::HashMap,
    path::{Component, PathBuf},
    sync::Arc,
};

mod import_edits;
mod index;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentLinkIndex {
    by_file: FxHashMap<PathBuf, Vec<StoredDocumentLink>>,
    source_contents: FxHashMap<PathBuf, Arc<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportEditPlan {
    pub(crate) changes: HashMap<Url, Vec<TextEdit>>,
    pub(crate) analyzed_contents: HashMap<Url, Arc<String>>,
}

#[derive(Clone, Debug)]
struct StoredDocumentLink {
    range: Range,
    directive_range: Range,
    import_path: String,
    import_style: ImportPathStyle,
    target: PathBuf,
}

#[derive(Clone, Debug)]
enum ImportPathStyle {
    Relative,
    Anchored {
        prefix: String,
        target_root: PathBuf,
        resolver_root: Option<PathBuf>,
        configuration_root: Option<PathBuf>,
        remappings: Arc<[ImportRemapping]>,
    },
    Opaque {
        resolver_root: Option<PathBuf>,
    },
}

fn components_to_import_path(components: &[Component<'_>]) -> String {
    let absolute = components.first().is_some_and(|component| *component == Component::RootDir);
    let path = components
        .iter()
        .filter(|component| !matches!(component, Component::RootDir | Component::CurDir))
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if absolute { format!("/{path}") } else { path }
}
