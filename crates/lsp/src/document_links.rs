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
