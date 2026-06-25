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

    pub(crate) fn remappings(&self) -> Vec<ImportRemapping> {
        self.remappings.clone()
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }
}
