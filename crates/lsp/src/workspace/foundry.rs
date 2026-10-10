use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};
use solar_interface::{data_structures::map::FxHashMap, source_map::apply_import_remappings};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

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
        dependency_config_roots: &mut Vec<PathBuf>,
    ) -> Vec<ImportRemapping> {
        let mut remappings = Vec::new();
        let mut authoritative = read_remappings_txt(root);
        if let Some(configured) = &self.remappings {
            authoritative.extend(configured.iter().cloned());
        }
        if self.auto_detect_remappings.unwrap_or(true) {
            remappings.extend(self.discover_lib_remappings(
                root,
                include_paths,
                &authoritative,
                dependency_config_roots,
            ));
            // Relative imports are normalized to source-unit names before remapping. Keep
            // dependency aliases that overlap the project's source namespaces contextual.
            let source_prefixes = self
                .build_source_roots(root)
                .iter()
                // Reserving an empty namespace would disable every dependency alias.
                .filter(|path| path.as_path() != root)
                .map(|path| remapping_path(root, path, true))
                .collect::<Vec<_>>();
            remappings.retain(|remapping| {
                #[cfg(windows)]
                let prefix = remapping.prefix.replace('\\', "/");
                #[cfg(not(windows))]
                let prefix = &remapping.prefix;
                !remapping.context.is_empty()
                    || !source_prefixes.iter().any(|source| {
                        source.starts_with(prefix.as_str()) || prefix.starts_with(source)
                    })
            });
            preserve_root_remappings(&mut remappings, &authoritative);
        }
        remappings.extend(authoritative);
        remappings
    }

    pub(crate) fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }

    fn discover_lib_remappings(
        &self,
        root: &Path,
        include_paths: &[PathBuf],
        authoritative: &[ImportRemapping],
        dependency_config_roots: &mut Vec<PathBuf>,
    ) -> Vec<ImportRemapping> {
        let mut remappings = Vec::<ImportRemapping>::new();
        let source_map = solar_interface::source_map::SourceMap::empty();
        let root_identity =
            source_map.file_loader().canonicalize_path(root).unwrap_or_else(|_| root.normalize());
        let mut ancestors = HashSet::from([root_identity]);
        for lib in include_paths {
            for package in directory_entries(lib) {
                if !package.is_dir() {
                    continue;
                }
                Self::discover_dependency_remappings(
                    root,
                    &package,
                    None,
                    &mut ancestors,
                    &mut remappings,
                    authoritative,
                    dependency_config_roots,
                );
            }
        }
        remappings.sort_by(|lhs, rhs| lhs.prefix.cmp(&rhs.prefix));
        remappings
    }

    /// Discovers remappings owned by a dependency and its nested library directories.
    ///
    /// Keep a global alias for root imports and a contextual refinement for each dependency.
    /// Explicit contexts remain scoped, and equal global aliases prefer the nearest package.
    fn discover_dependency_remappings(
        root: &Path,
        dependency: &Path,
        parent_context: Option<&str>,
        ancestors: &mut HashSet<PathBuf>,
        remappings: &mut Vec<ImportRemapping>,
        authoritative: &[ImportRemapping],
        dependency_config_roots: &mut Vec<PathBuf>,
    ) {
        let source_map = solar_interface::source_map::SourceMap::empty();
        let identity = source_map
            .file_loader()
            .canonicalize_path(dependency)
            .unwrap_or_else(|_| dependency.normalize());
        // Guard cycles along the current path, while retaining separate lexical aliases of the
        // same physical package reached through other configured library entries.
        if !ancestors.insert(identity.clone()) {
            return;
        }

        // Both config files are inputs even when absent, so creating either must rediscover.
        dependency_config_roots.push(dependency.normalize());

        let name = dependency.file_name().and_then(|name| name.to_str());
        let context = remapping_path(root, dependency, true);
        let profile = load_nested_profile(dependency);
        let source = profile
            .as_ref()
            .and_then(|profile| profile.src.as_deref())
            .unwrap_or_else(|| Path::new("src"));
        let source = dependency.join(source).normalize();
        if let Some(name) = name
            && source.is_dir()
        {
            let global = ImportRemapping {
                context: String::new(),
                prefix: format!("{name}/"),
                path: remapping_path(root, &source, true),
            };
            if let Some(parent_context) = parent_context {
                let mut contextual = global.clone();
                contextual.context = parent_context.to_owned();
                remappings.push(contextual);
            }
            if parent_context.is_none() || !root_overrides_global(authoritative, &global.prefix) {
                push_closest_global(remappings, global);
            }
        }

        let nested_include_paths = profile
            .as_ref()
            .map(|profile| profile.include_paths(dependency))
            .unwrap_or_else(|| vec![dependency.join("lib")]);
        for include_path in nested_include_paths {
            for package in directory_entries(&include_path) {
                if !package.is_dir() || is_symlink(&package) {
                    continue;
                }
                Self::discover_dependency_remappings(
                    root,
                    &package,
                    Some(&context),
                    ancestors,
                    remappings,
                    authoritative,
                    dependency_config_roots,
                );
            }
        }

        // Dependency-local declarations are authoritative over the automatically discovered
        // package roots in that dependency. Append them after nested auto mappings so the normal
        // remapping selection rule gives an equal-context declaration precedence.
        let mut declared_remappings = read_remappings_txt(dependency);
        if let Some(profile) = &profile
            && let Some(configured) = &profile.remappings
        {
            declared_remappings.extend(configured.iter().cloned());
        }
        for remapping in declared_remappings {
            let is_global = remapping.context.is_empty();
            let Some(remapping) = rebase_nested_remapping(root, dependency, &context, remapping)
            else {
                continue;
            };
            let has_global_alias = remappings.iter().any(|candidate| {
                candidate.context.is_empty() && candidate.prefix == remapping.prefix
            });
            if !has_global_alias
                && is_global
                && !root_overrides_global(authoritative, &remapping.prefix)
            {
                let mut global = remapping.clone();
                global.context.clear();
                remappings.push(global);
            }
            remappings.push(remapping);
        }
        ancestors.remove(&identity);
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

fn directory_entries(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else { return Vec::new() };
    let mut entries = entries.filter_map(Result::ok).map(|entry| entry.path()).collect::<Vec<_>>();
    entries.sort();
    entries
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn load_nested_profile(root: &Path) -> Option<FoundryProfile> {
    let path = root.join("foundry.toml");
    let source_map = solar_interface::source_map::SourceMap::empty();
    let contents = source_map.file_loader().load_file(&path).ok()?;
    // NOTE: Dependency profiles default locally; hosts can supply fully resolved configuration
    // for profile- and environment-specific behavior.
    toml_edit::de::from_str::<FoundryDocument>(&contents).ok().map(|doc| doc.profile_for(None))
}

fn push_unique(remappings: &mut Vec<ImportRemapping>, remapping: ImportRemapping) {
    if !remappings.iter().any(|candidate| {
        candidate.context == remapping.context
            && candidate.prefix == remapping.prefix
            && candidate.path == remapping.path
    }) {
        remappings.push(remapping);
    }
}

fn remapping_path(root: &Path, path: &Path, trailing_separator: bool) -> String {
    let path = path.normalize();
    let path = path.strip_prefix(root).unwrap_or(&path);
    let mut path = path.to_string_lossy().replace('\\', "/");
    if trailing_separator && !path.ends_with('/') {
        path.push('/');
    }
    path
}

fn rebase_nested_remapping(
    root: &Path,
    dependency: &Path,
    dependency_context: &str,
    mut remapping: ImportRemapping,
) -> Option<ImportRemapping> {
    let target_has_separator = remapping.path.ends_with(['/', '\\']);
    let target = Path::new(&remapping.path);
    let target = if target.is_absolute() { target.to_path_buf() } else { dependency.join(target) };
    remapping.path = remapping_path(root, &target, target_has_separator);

    let context_has_separator = remapping.context.ends_with(['/', '\\']);
    let context = if remapping.context.is_empty() {
        PathBuf::from(dependency_context)
    } else {
        let context = Path::new(&remapping.context);
        if context.is_absolute() { context.to_path_buf() } else { dependency.join(context) }
    };
    remapping.context =
        remapping_path(root, &context, context_has_separator || remapping.context.is_empty());
    if remapping.context.is_empty() {
        return None;
    }
    Some(remapping)
}

fn push_closest_global(remappings: &mut Vec<ImportRemapping>, remapping: ImportRemapping) {
    if let Some(existing) = remappings
        .iter_mut()
        .find(|candidate| candidate.context.is_empty() && candidate.prefix == remapping.prefix)
    {
        if Path::new(&remapping.path).components().count()
            < Path::new(&existing.path).components().count()
        {
            *existing = remapping;
        }
    } else {
        remappings.push(remapping);
    }
}

/// Keeps root declarations authoritative despite the more specific dependency contexts.
fn preserve_root_remappings(
    remappings: &mut Vec<ImportRemapping>,
    authoritative: &[ImportRemapping],
) {
    let mut overlays = Vec::new();
    remappings.retain(|remapping| {
        // Root auto aliases retain their existing selection behavior. Newly discovered global
        // aliases were already checked against root declarations when they were collected.
        if remapping.context.is_empty() {
            return true;
        }
        let applicable =
            |candidate: &&ImportRemapping| remapping.context.starts_with(&candidate.context);
        if authoritative
            .iter()
            .filter(applicable)
            .any(|candidate| remapping.prefix.starts_with(&candidate.prefix))
        {
            return false;
        }
        for candidate in authoritative
            .iter()
            .filter(applicable)
            .filter(|candidate| candidate.prefix.starts_with(&remapping.prefix))
        {
            // A narrower root alias would otherwise lose to the dependency context. Resolve
            // its target with the original root rules before copying it into that context,
            // preserving precedence between root declarations with different contexts.
            let mut overlay = candidate.clone();
            overlay.path = apply_import_remappings(
                authoritative,
                Path::new(&candidate.prefix),
                Some(Path::new(&remapping.context)),
            )
            .to_string_lossy()
            .into_owned();
            overlay.context.clone_from(&remapping.context);
            push_unique(&mut overlays, overlay);
        }
        true
    });
    remappings.extend(overlays);
}

fn root_overrides_global(authoritative: &[ImportRemapping], prefix: &str) -> bool {
    authoritative
        .iter()
        .any(|candidate| candidate.context.is_empty() && prefix.starts_with(&candidate.prefix))
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
        assert!(profile.remappings_with_include_paths(root, &[], &mut Vec::new()).is_empty());
        assert_eq!(
            document
                .profile_for(Some("missing"))
                .remappings_with_include_paths(root, &[], &mut Vec::new())
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

    #[test]
    fn nested_dependency_remappings_are_rebased_and_contextual() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        std::fs::create_dir_all(dependency.join("src")).unwrap();
        std::fs::write(dependency.join("remappings.txt"), "y/=lib/y/src/\n").unwrap();

        let remappings = FoundryProfile::default()
            .remappings_with_include_paths(root, &[root.join("lib")], &mut Vec::new())
            .into_iter()
            .map(|remapping| remapping.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            remappings,
            ["x/=lib/x/src/", "y/=lib/x/lib/y/src/", "lib/x/:y/=lib/x/lib/y/src/"]
        );
    }

    #[test]
    fn nested_foundry_profile_remappings_use_dependency_paths() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        std::fs::create_dir_all(dependency.join("contracts")).unwrap();
        std::fs::write(
            dependency.join("foundry.toml"),
            "[profile.default]\nsrc = \"contracts\"\nremappings = [\"@dep/=contracts/\"]\n",
        )
        .unwrap();

        let remappings = FoundryProfile::default()
            .remappings_with_include_paths(root, &[root.join("lib")], &mut Vec::new())
            .into_iter()
            .map(|remapping| remapping.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            remappings,
            ["@dep/=lib/x/contracts/", "lib/x/:@dep/=lib/x/contracts/", "x/=lib/x/contracts/"]
        );
    }

    #[test]
    fn nested_explicit_context_does_not_create_a_global_alias() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::write(dependency.join("remappings.txt"), "src/:y/=private/\n").unwrap();
        let remappings = FoundryProfile::default().remappings_with_include_paths(
            root,
            &[root.join("lib")],
            &mut Vec::new(),
        );

        for (importer, expected) in [
            ("lib/x/src/X.sol", "lib/x/private/Y.sol"),
            ("lib/x/test/X.sol", "y/Y.sol"),
            ("lib/other/src/X.sol", "y/Y.sol"),
            ("src/Main.sol", "y/Y.sol"),
        ] {
            assert_eq!(
                apply_import_remappings(
                    &remappings,
                    Path::new("y/Y.sol"),
                    Some(Path::new(importer))
                ),
                Path::new(expected),
                "importer: {importer}",
            );
        }
    }

    #[test]
    fn root_remappings_remain_authoritative_in_nested_contexts() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        fs::create_dir_all(dependency.join("lib/y/src")).unwrap();
        fs::create_dir_all(dependency.join("lib/z/lib/y/src")).unwrap();
        fs::write(dependency.join("remappings.txt"), "y/special/=private/\n").unwrap();

        for (configured, import, expected) in [
            ("y/=local/", "y/Y.sol", "local/Y.sol"),
            ("y/=local/", "y/special/Y.sol", "local/special/Y.sol"),
            ("y/special/=local/", "y/special/Y.sol", "local/Y.sol"),
            ("lib/x/:y/=local/", "y/Y.sol", "local/Y.sol"),
            ("lib/x/:y/=local/", "y/special/Y.sol", "local/special/Y.sol"),
            ("lib/x/:y/special/=local/", "y/special/Y.sol", "local/Y.sol"),
            (
                "y/special/deep/=global/\nlib/x/:y/special/=local/",
                "y/special/deep/Y.sol",
                "local/deep/Y.sol",
            ),
        ] {
            let profile = FoundryProfile {
                remappings: Some(configured.lines().map(|line| line.parse().unwrap()).collect()),
                ..Default::default()
            };
            let remappings =
                profile.remappings_with_include_paths(root, &[root.join("lib")], &mut Vec::new());
            for importer in ["lib/x/src/X.sol", "lib/x/lib/z/src/Z.sol"] {
                assert_eq!(
                    apply_import_remappings(
                        &remappings,
                        Path::new(import),
                        Some(Path::new(importer)),
                    ),
                    Path::new(expected),
                    "configured: {configured}, importer: {importer}",
                );
            }
        }
    }

    #[test]
    fn nested_configuration_can_restore_an_automatic_target() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        fs::create_dir_all(dependency.join("lib/y/src")).unwrap();
        fs::write(dependency.join("remappings.txt"), "y/=private/\n").unwrap();
        fs::write(
            dependency.join("foundry.toml"),
            "[profile.default]\nremappings = [\"y/=lib/y/src/\"]\n",
        )
        .unwrap();
        let remappings = FoundryProfile::default().remappings_with_include_paths(
            root,
            &[root.join("lib")],
            &mut Vec::new(),
        );

        assert_eq!(
            apply_import_remappings(
                &remappings,
                Path::new("y/Y.sol"),
                Some(Path::new("lib/x/src/X.sol")),
            ),
            Path::new("lib/x/lib/y/src/Y.sol"),
        );
    }

    #[test]
    fn nested_library_paths_cannot_revisit_ancestor_packages() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let dependency = root.join("lib/x");
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::write(dependency.join("foundry.toml"), "[profile.default]\nlibs = [\"..\"]\n").unwrap();
        let remappings = FoundryProfile::default().remappings_with_include_paths(
            root,
            &[root.join("lib")],
            &mut Vec::new(),
        );

        assert_eq!(remappings.len(), 1);
        assert_eq!(remappings[0].prefix, "x/");
        assert_eq!(remappings[0].context, "");
        assert_eq!(remappings[0].path, "lib/x/src/");
    }

    #[test]
    fn source_namespaces_preserve_unrelated_and_explicit_root_aliases() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().join("workspace");
        let dependency = root.join("lib/x");
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::write(dependency.join("remappings.txt"), "src/=src/\nsrc2/=src/\n").unwrap();

        for src in ["src", ".", "../external"] {
            let profile = FoundryProfile {
                src: Some(src.into()),
                remappings: Some(vec!["src/=local/".parse().unwrap()]),
                ..Default::default()
            };
            let remappings =
                profile.remappings_with_include_paths(&root, &[root.join("lib")], &mut Vec::new());
            for (import, expected) in [
                ("x/X.sol", "lib/x/src/X.sol"),
                ("src2/X.sol", "lib/x/src/X.sol"),
                ("src/X.sol", "local/X.sol"),
            ] {
                assert_eq!(
                    apply_import_remappings(
                        &remappings,
                        Path::new(import),
                        Some(Path::new("Main.sol"))
                    ),
                    Path::new(expected),
                    "src={src}, import={import}",
                );
            }
        }
    }
}
