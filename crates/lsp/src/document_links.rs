use lsp_types::{Range, TextEdit, Url};
use solar_config::ImportRemapping;
use solar_interface::data_structures::map::FxHashMap;
use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[cfg(windows)]
use std::path::Prefix;
#[cfg(unix)]
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
};

mod import_edits;
mod index;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentLinkIndex {
    by_file: FxHashMap<PathBuf, IndexedDocumentLinks>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportEditPlan {
    by_file: HashMap<Url, PlannedImportEdits>,
}

#[derive(Clone, Debug)]
struct IndexedDocumentLinks {
    analyzed_contents: Arc<String>,
    links: Vec<StoredDocumentLink>,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedImportEdits {
    analyzed_contents: Arc<String>,
    edits: Vec<TextEdit>,
}

#[derive(Clone, Debug)]
struct StoredDocumentLink {
    range: Range,
    directive_range: Range,
    import_path: PathBuf,
    import_style: ImportPathStyle,
    target: PathBuf,
}

#[derive(Clone, Debug)]
enum ImportPathStyle {
    Relative {
        resolver_root: Option<PathBuf>,
        remappings: Arc<[ImportRemapping]>,
    },
    Anchored {
        prefix: PathBuf,
        target_root: PathBuf,
        resolver_root: Option<PathBuf>,
        configuration_root: Option<PathBuf>,
        remappings: Arc<[ImportRemapping]>,
    },
    Opaque {
        resolver_root: Option<PathBuf>,
        remappings: Arc<[ImportRemapping]>,
    },
}

impl ImportEditPlan {
    fn push(&mut self, uri: Url, analyzed_contents: Arc<String>, edit: TextEdit) {
        let planned = self.by_file.entry(uri).or_insert_with(|| PlannedImportEdits {
            analyzed_contents: analyzed_contents.clone(),
            edits: Vec::new(),
        });
        debug_assert_eq!(planned.analyzed_contents, analyzed_contents);
        planned.edits.push(edit);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_file.is_empty()
    }

    pub(crate) fn retain(&mut self, mut predicate: impl FnMut(&Url) -> bool) {
        self.by_file.retain(|uri, _| predicate(uri));
    }

    pub(crate) fn into_entries(self) -> impl Iterator<Item = (Url, PlannedImportEdits)> {
        self.by_file.into_iter()
    }

    #[cfg(test)]
    pub(crate) fn changes(&self) -> HashMap<Url, Vec<TextEdit>> {
        self.by_file.iter().map(|(uri, planned)| (uri.clone(), planned.edits.clone())).collect()
    }

    #[cfg(test)]
    pub(crate) fn first_edit(&self) -> Option<&TextEdit> {
        self.by_file.values().flat_map(|planned| &planned.edits).next()
    }
}

impl PlannedImportEdits {
    pub(crate) fn into_parts(self) -> (Arc<String>, Vec<TextEdit>) {
        (self.analyzed_contents, self.edits)
    }
}

fn import_path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes).ok().map(PathBuf::from)
    }
}

fn import_path_bytes(path: &Path) -> Cow<'_, [u8]> {
    #[cfg(unix)]
    {
        Cow::Borrowed(path.as_os_str().as_bytes())
    }
    #[cfg(windows)]
    {
        let bytes = path.as_os_str().as_encoded_bytes();
        let verbatim = path.components().next().is_some_and(|component| {
            matches!(
                component,
                Component::Prefix(prefix)
                    if matches!(
                        prefix.kind(),
                        Prefix::Verbatim(_)
                            | Prefix::VerbatimUNC(..)
                            | Prefix::VerbatimDisk(_)
                            | Prefix::DeviceNS(_)
                    )
            )
        });
        if verbatim {
            return Cow::Borrowed(bytes);
        }
        Cow::Owned(bytes.iter().map(|&byte| if byte == b'\\' { b'/' } else { byte }).collect())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Cow::Borrowed(path.as_os_str().as_encoded_bytes())
    }
}

fn components_to_import_path(components: &[Component<'_>]) -> PathBuf {
    components
        .iter()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| component.as_os_str())
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn import_path_preserves_non_utf8_bytes() {
        let expected = b"./Target-\xff.sol";
        let path = import_path_from_bytes(expected).unwrap();

        assert_eq!(import_path_bytes(&path).as_ref(), expected);
    }
}
