use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FoundryDocument {
    profile: Option<FoundryProfiles>,
    default: Option<FoundryProfile>,
}

impl FoundryDocument {
    #[cfg(test)]
    pub(crate) fn default_profile(&self) -> FoundryProfile {
        self.profile_for(None)
    }

    pub(crate) fn profile_for(&self, selected_profile: Option<&str>) -> FoundryProfile {
        let default = self.base_profile();
        let Some(name) = selected_profile.filter(|name| *name != "default") else {
            return default;
        };
        self.profile
            .as_ref()
            .and_then(|profiles| profiles.get(name))
            .map_or(default.clone(), |profile| default.overlay(&profile))
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

#[derive(Clone, Debug, Default, Deserialize)]
struct FoundryProfiles {
    default: Option<FoundryProfile>,
    #[serde(flatten)]
    profiles: BTreeMap<String, serde_json::Value>,
}

impl FoundryProfiles {
    fn get(&self, name: &str) -> Option<FoundryProfile> {
        self.profiles.get(name).cloned().and_then(|profile| serde_json::from_value(profile).ok())
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
    fn overlay(&self, overlay: &Self) -> Self {
        Self {
            src: overlay.src.clone().or_else(|| self.src.clone()),
            test: overlay.test.clone().or_else(|| self.test.clone()),
            script: overlay.script.clone().or_else(|| self.script.clone()),
            libs: overlay.libs.clone().or_else(|| self.libs.clone()),
            auto_detect_remappings: overlay.auto_detect_remappings.or(self.auto_detect_remappings),
            remappings: overlay.remappings.clone().or_else(|| self.remappings.clone()),
            evm_version: overlay.evm_version.or(self.evm_version),
        }
    }

    pub(crate) fn source_roots(&self, root: &Path) -> Vec<PathBuf> {
        vec![root.join(self.src.as_deref().unwrap_or_else(|| Path::new("src"))).normalize()]
    }

    pub(crate) fn flycheck_source_roots(&self, root: &Path) -> Vec<PathBuf> {
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
            remappings.extend(configured.clone());
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

    #[test]
    fn document_reports_configured_include_paths_directly() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [profile.default]
            libs = ["lib", "vendor"]
            auto_detect_remappings = true
            remappings = ["@example/=vendor/example/src/"]
            "#,
        )
        .unwrap();

        assert_eq!(
            document.default_profile().include_paths(Path::new("workspace")),
            [PathBuf::from("workspace/lib"), PathBuf::from("workspace/vendor")]
        );
    }

    #[test]
    fn selected_profile_overlays_default_profile_fields() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
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
        )
        .unwrap();

        let profile = document.profile_for(Some("custom"));
        assert_eq!(
            profile.source_roots(Path::new("workspace")),
            [PathBuf::from("workspace/custom-src")]
        );
        assert_eq!(
            profile.flycheck_source_roots(Path::new("workspace")),
            [
                PathBuf::from("workspace/custom-src"),
                PathBuf::from("workspace/default-test"),
                PathBuf::from("workspace/default-script"),
            ]
        );
        assert_eq!(
            profile.include_paths(Path::new("workspace")),
            [PathBuf::from("workspace/default-libs")]
        );
        assert!(profile.remappings.as_ref().is_some_and(Vec::is_empty));
        assert_eq!(profile.auto_detect_remappings, Some(false));
        assert_eq!(profile.evm_version(), Some(EvmVersion::Paris));
    }

    #[test]
    fn missing_and_default_profiles_use_default_profile() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [profile.default]
            src = "default-src"

            [profile.custom]
            src = "custom-src"
            "#,
        )
        .unwrap();

        let default_roots = document.default_profile().source_roots(Path::new("workspace"));
        assert_eq!(document.profile_for(None).source_roots(Path::new("workspace")), default_roots);
        assert_eq!(
            document.profile_for(Some("default")).source_roots(Path::new("workspace")),
            default_roots
        );
        assert_eq!(
            document.profile_for(Some("missing")).source_roots(Path::new("workspace")),
            default_roots
        );
    }

    #[test]
    fn legacy_default_profile_remains_supported() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [default]
            src = "legacy-src"
            "#,
        )
        .unwrap();

        assert_eq!(
            document.default_profile().source_roots(Path::new("workspace")),
            [PathBuf::from("workspace/legacy-src")]
        );
        assert_eq!(
            document.profile_for(Some("missing")).source_roots(Path::new("workspace")),
            [PathBuf::from("workspace/legacy-src")]
        );
    }

    #[test]
    fn unselected_profile_does_not_affect_default_profile_parsing() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [profile.default]
            src = "default-src"

            [profile.unselected]
            evm_version = "future-hardfork"
            "#,
        )
        .unwrap();

        assert_eq!(
            document.default_profile().source_roots(Path::new("workspace")),
            [PathBuf::from("workspace/default-src")]
        );
    }

    #[test]
    fn explicit_empty_remappings_replace_default_remappings() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [profile.default]
            remappings = ["default/=default/src/"]

            [profile.custom]
            remappings = []
            "#,
        )
        .unwrap();
        let profile = document.profile_for(Some("custom"));

        assert!(profile.remappings_with_include_paths(Path::new("workspace"), &[]).is_empty());
        assert_eq!(
            document
                .profile_for(Some("missing"))
                .remappings_with_include_paths(Path::new("workspace"), &[])
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["default/=default/src/"]
        );
    }
}
