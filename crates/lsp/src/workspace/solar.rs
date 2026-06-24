use std::path::{Path, PathBuf};

use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use solar_interface::source_map::SourceMap;

use super::WorkspaceError;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SolarDocument {
    compiler: Option<SolarCompilerConfig>,
}

impl SolarDocument {
    pub(crate) fn load(path: &Path) -> Result<Self, WorkspaceError> {
        let source_map = SourceMap::empty();
        let contents = source_map
            .file_loader()
            .load_file(path)
            .map_err(|source| WorkspaceError::Read { path: path.to_path_buf(), source })?;
        toml_edit::de::from_str(&contents)
            .map_err(|source| WorkspaceError::ParseToml { path: path.to_path_buf(), source })
    }

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
    #[serde(default, with = "optional_display_fromstr")]
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

mod optional_display_fromstr {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Deserializer, de};

    pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| value.parse().map_err(de::Error::custom))
            .transpose()
    }
}
