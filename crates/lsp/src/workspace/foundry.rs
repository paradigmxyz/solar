use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct FoundryDocument {
    #[serde(default)]
    profile: BTreeMap<String, FoundryProfile>,
    default: Option<FoundryProfile>,
    #[serde(default)]
    fmt: FoundryFormatterConfig,
}

impl FoundryDocument {
    pub(crate) fn default_profile(mut self) -> FoundryProfile {
        self.profile.remove("default").or(self.default).unwrap_or_default()
    }

    pub(crate) fn formatter_ignores(&self, profile: &str) -> &[String] {
        self.profile(profile)
            .and_then(|profile| profile.fmt.ignore.as_deref())
            .or(self.fmt.ignore.as_deref())
            .unwrap_or(&[])
    }

    fn profile(&self, profile: &str) -> Option<&FoundryProfile> {
        self.profile
            .get(profile)
            .or_else(|| (profile == "default").then_some(self.default.as_ref()).flatten())
    }
}

#[derive(Debug, Default, Deserialize)]
struct FoundryFormatterConfig {
    ignore: Option<Vec<String>>,
}

/// A subset of Foundry config relevant to LSP compilation.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct FoundryProfile {
    src: Option<PathBuf>,
    libs: Option<Vec<PathBuf>>,
    auto_detect_remappings: Option<bool>,
    #[serde(default, with = "crate::serde::display_fromstr::vec")]
    remappings: Vec<ImportRemapping>,
    #[serde(default, with = "crate::serde::optional_display_fromstr")]
    evm_version: Option<EvmVersion>,
    #[serde(default)]
    fmt: FoundryFormatterConfig,
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
        let mut remappings = Vec::new();
        if self.auto_detect_remappings.unwrap_or(true) {
            remappings.extend(self.discover_lib_remappings(root));
        }
        remappings.extend(read_remappings_txt(root));
        remappings.extend(self.remappings.clone());
        remappings
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }

    fn discover_lib_remappings(&self, root: &Path) -> Vec<ImportRemapping> {
        let mut remappings = Vec::<ImportRemapping>::new();
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
    fn formatter_ignores_resolve_selected_default_profile() {
        let document = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [fmt]
            ignore = ["root.sol"]

            [profile.default.fmt]
            ignore = ["default.sol"]

            [profile.ci.fmt]
            ignore = ["ci.sol"]

            [profile.inherit.fmt]
            line_length = 88

            [profile.clear.fmt]
            ignore = []
            "#,
        )
        .unwrap();

        assert_eq!(document.formatter_ignores("default"), ["default.sol"]);
        assert_eq!(document.formatter_ignores("ci"), ["ci.sol"]);
        assert_eq!(document.formatter_ignores("inherit"), ["root.sol"]);
        assert!(document.formatter_ignores("clear").is_empty());
        assert_eq!(document.formatter_ignores("unknown"), ["root.sol"]);
    }

    #[test]
    fn formatter_ignores_use_legacy_default_only_without_standard_default() {
        let legacy = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [fmt]
            ignore = ["root.sol"]

            [default.fmt]
            ignore = ["legacy.sol"]
            "#,
        )
        .unwrap();
        assert_eq!(legacy.formatter_ignores("default"), ["legacy.sol"]);

        let standard = toml_edit::de::from_str::<FoundryDocument>(
            r#"
            [fmt]
            ignore = ["root.sol"]

            [default.fmt]
            ignore = ["legacy.sol"]

            [profile.default.fmt]
            line_length = 88
            "#,
        )
        .unwrap();
        assert_eq!(standard.formatter_ignores("default"), ["root.sol"]);
    }
}
