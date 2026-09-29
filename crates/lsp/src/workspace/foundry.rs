use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use solar_interface::data_structures::map::FxHashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct FoundryDocument {
    profile: Option<FoundryProfiles>,
    default: Option<FoundryProfile>,
}

impl FoundryDocument {
    pub(crate) fn profile_for(&self, selected_profile: Option<&str>) -> FoundryProfile {
        let default = self.base_profile();
        let Some(name) = selected_profile.filter(|name| *name != "default") else {
            return default;
        };
        let Some(profile) = self.profile.as_ref().and_then(|profiles| profiles.get(name)) else {
            return default;
        };
        default.overlay(profile)
    }

    fn base_profile(&self) -> FoundryProfile {
        self.profile
            .as_ref()
            .and_then(|profiles| profiles.default.as_ref())
            .cloned()
            .or_else(|| self.default.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Default, Deserialize)]
struct FoundryProfiles {
    default: Option<FoundryProfile>,
    #[serde(flatten)]
    profiles: FxHashMap<String, serde_json::Value>,
}

impl FoundryProfiles {
    fn get(&self, name: &str) -> Option<FoundryProfile> {
        self.profiles.get(name).and_then(|profile| FoundryProfile::deserialize(profile).ok())
    }
}

/// A subset of Foundry config relevant to the LSP workspace.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FoundryProfile {
    src: Option<PathBuf>,
    test: Option<PathBuf>,
    script: Option<PathBuf>,
    libs: Option<Vec<PathBuf>>,
    auto_detect_remappings: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::serde::optional_display_fromstr::vec::deserialize"
    )]
    remappings: Option<Vec<ImportRemapping>>,
    #[serde(default, with = "crate::serde::optional_display_fromstr")]
    evm_version: Option<EvmVersion>,
}

impl FoundryProfile {
    fn overlay(self, overlay: Self) -> Self {
        Self {
            src: overlay.src.or(self.src),
            test: overlay.test.or(self.test),
            script: overlay.script.or(self.script),
            libs: overlay.libs.or(self.libs),
            auto_detect_remappings: overlay.auto_detect_remappings.or(self.auto_detect_remappings),
            remappings: overlay.remappings.or(self.remappings),
            evm_version: overlay.evm_version.or(self.evm_version),
        }
    }

    pub(crate) fn build_source_roots(&self, root: &Path) -> Vec<PathBuf> {
        [
            self.src.as_deref().unwrap_or_else(|| Path::new("src")),
            self.test.as_deref().unwrap_or_else(|| Path::new("test")),
            self.script.as_deref().unwrap_or_else(|| Path::new("script")),
        ]
        .into_iter()
        .map(|path| root.join(path).normalize())
        .collect()
    }

    pub(crate) fn include_paths(&self, root: &Path) -> Vec<PathBuf> {
        match &self.libs {
            Some(libs) => libs.iter().map(|path| root.join(path)).collect(),
            None => vec![root.join("lib")],
        }
    }

    pub(crate) fn remappings_with_include_paths(
        &self,
        root: &Path,
        include_paths: &[PathBuf],
    ) -> Vec<ImportRemapping> {
        let mut remappings = Vec::new();
        if self.auto_detect_remappings.unwrap_or(true) {
            remappings.extend(self.discover_lib_remappings(root, include_paths));
        }
        remappings.extend(read_remappings_txt(root));
        if let Some(configured) = &self.remappings {
            remappings.extend_from_slice(configured);
        }
        remappings
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }

    fn discover_lib_remappings(
        &self,
        root: &Path,
        include_paths: &[PathBuf],
    ) -> Vec<ImportRemapping> {
        let mut remappings = Vec::<ImportRemapping>::new();
        for lib in include_paths {
            let Ok(entries) = std::fs::read_dir(lib) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let package = entry.path();
                let src = package.join("src");
                if src.is_dir()
                    && let Some(name) = package.file_name().and_then(|name| name.to_str())
                    && let Some(path) = src.strip_prefix(root).unwrap_or(&src).to_str()
                    && let Ok(remapping) = format!("{name}/={}/", path.replace('\\', "/")).parse()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn document(toml: &str) -> FoundryDocument {
        toml_edit::de::from_str(toml).unwrap()
    }

    fn workspace_paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(|path| Path::new("workspace").join(path)).collect()
    }

    #[test]
    fn selected_profile_overlays_default_profile_fields() {
        let document = document(
            r#"
            [profile.default]
            src = "default-src"
            test = "default-test"
            script = "default-script"
            libs = ["default-libs"]
            auto_detect_remappings = false
            remappings = ["default/=default/src/"]
            evm_version = "paris"

            [profile.custom]
            src = "custom-src"
            remappings = []
            "#,
        );
        let root = Path::new("workspace");

        let profile = document.profile_for(Some("custom"));
        assert_eq!(
            profile.build_source_roots(root),
            workspace_paths(&["custom-src", "default-test", "default-script"])
        );
        assert_eq!(profile.include_paths(root), workspace_paths(&["default-libs"]));
        assert_eq!(profile.auto_detect_remappings, Some(false));
        assert_eq!(profile.evm_version(), Some(EvmVersion::Paris));
        assert!(profile.remappings_with_include_paths(root, &[]).is_empty());
        assert_eq!(
            document
                .profile_for(Some("missing"))
                .remappings_with_include_paths(root, &[])
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["default/=default/src/"]
        );
    }

    #[test]
    fn unselected_missing_and_invalid_profiles_use_default_profile() {
        for (toml, src) in [
            (
                r#"
                [profile.default]
                src = "default-src"

                [profile.custom]
                src = "custom-src"

                [profile.invalid]
                evm_version = "future-hardfork"
                "#,
                "default-src",
            ),
            ("[default]\nsrc = \"legacy-src\"\n", "legacy-src"),
        ] {
            let document = document(toml);
            for profile in [None, Some("default"), Some("missing"), Some("invalid")] {
                assert_eq!(
                    document.profile_for(profile).build_source_roots(Path::new("workspace")),
                    workspace_paths(&[src, "test", "script"]),
                    "{profile:?} in {toml}"
                );
            }
        }
    }
}
