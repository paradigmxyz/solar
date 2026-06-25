use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SolarDocument {
    compiler: Option<SolarCompilerConfig>,
}

impl SolarDocument {
    pub(crate) fn compiler(self) -> SolarCompilerConfig {
        self.compiler.unwrap_or_default()
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SolarCompilerConfig {
    base_path: Option<PathBuf>,
    source_paths: Option<Vec<PathBuf>>,
    include_paths: Option<Vec<PathBuf>>,
    #[serde(default, with = "crate::serde::display_fromstr::vec")]
    remappings: Vec<ImportRemapping>,
    #[serde(default, with = "crate::serde::optional_display_fromstr")]
    evm_version: Option<EvmVersion>,
}

impl SolarCompilerConfig {
    pub(crate) fn base_path(&self, manifest_root: &Path) -> PathBuf {
        self.base_path
            .as_deref()
            .map(|path| join_path(manifest_root, path))
            .unwrap_or_else(|| manifest_root.to_path_buf())
    }

    pub(crate) fn source_roots(&self, base_path: &Path) -> Vec<PathBuf> {
        match &self.source_paths {
            Some(source_paths) => {
                source_paths.iter().map(|path| join_path(base_path, path)).collect()
            }
            None => vec![base_path.join("src")],
        }
    }

    pub(crate) fn include_paths(&self, base_path: &Path) -> Vec<PathBuf> {
        self.include_paths
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|path| join_path(base_path, path))
            .collect()
    }

    pub(crate) fn remappings(&self) -> Vec<ImportRemapping> {
        self.remappings.clone()
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }
}

fn join_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { root.join(path) }
}
