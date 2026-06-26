use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct FoundryDocument {
    profile: Option<FoundryProfiles>,
    default: Option<FoundryProfile>,
}

impl FoundryDocument {
    pub(crate) fn default_profile(self) -> FoundryProfile {
        self.profile.and_then(|profiles| profiles.default).or(self.default).unwrap_or_default()
    }
}

#[derive(Debug, Default, Deserialize)]
struct FoundryProfiles {
    default: Option<FoundryProfile>,
}

/// A subset of Foundry config relevant to LSP compilation.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct FoundryProfile {
    src: Option<PathBuf>,
    libs: Option<Vec<PathBuf>>,
    #[serde(default, with = "crate::serde::display_fromstr::vec")]
    remappings: Vec<ImportRemapping>,
    #[serde(default, with = "crate::serde::optional_display_fromstr")]
    evm_version: Option<EvmVersion>,
}

impl FoundryProfile {
    pub(crate) fn source_roots(&self, root: &Path) -> Vec<PathBuf> {
        vec![root.join(self.src.as_deref().unwrap_or_else(|| Path::new("src")))]
    }

    pub(crate) fn include_paths(&self, root: &Path) -> Vec<PathBuf> {
        match &self.libs {
            Some(libs) => libs.iter().map(|path| root.join(path)).collect(),
            None => vec![root.join("lib")],
        }
    }

    pub(crate) fn remappings(&self, root: &Path) -> Vec<ImportRemapping> {
        let mut remappings = self.discover_lib_remappings(root);
        remappings.extend(read_remappings_txt(root));
        remappings.extend(self.remappings.clone());
        remappings
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }

    fn discover_lib_remappings(&self, root: &Path) -> Vec<ImportRemapping> {
        let mut remappings: Vec<ImportRemapping> = Vec::new();
        for lib in self.include_paths(root) {
            let Ok(entries) = std::fs::read_dir(&lib) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let package = entry.path();
                let src = package.join("src");
                if src.is_dir()
                    && let Some(name) = package.file_name().and_then(|name| name.to_str())
                    && let Some(path) = src.strip_prefix(root).ok().and_then(Path::to_str)
                    && let Ok(remapping) = format!("{name}/={path}/").parse()
                {
                    remappings.push(remapping);
                }
            }
        }
        remappings.sort_by(|lhs, rhs| lhs.prefix.cmp(&rhs.prefix));
        remappings
    }
}

fn read_remappings_txt(root: &Path) -> Vec<ImportRemapping> {
    let path = root.join("remappings.txt");
    let source_map = solar_interface::source_map::SourceMap::empty();
    let Ok(contents) = source_map.file_loader().load_file(&path) else {
        return Vec::new();
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.parse().ok())
        .collect()
}
