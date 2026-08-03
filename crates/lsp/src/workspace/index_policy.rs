//! Workspace indexing boundaries and exclusion rules.

use glob::{MatchOptions, Pattern};
use serde::Deserialize;
use std::{
    path::{Component, Path},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const DEFAULT_EXCLUDED_DIRECTORIES: &[&str] =
    &["artifacts", "broadcast", "build", "cache", "dist", "lib", "node_modules", "out", "target"];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct IndexingOptions {
    pub(crate) exclude: Vec<String>,
    pub(crate) use_default_excludes: bool,
    pub(crate) exclude_hidden_directories: bool,
    pub(crate) exclude_nested_repositories: bool,
}

impl Default for IndexingOptions {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            use_default_excludes: true,
            exclude_hidden_directories: true,
            exclude_nested_repositories: true,
        }
    }
}

impl IndexingOptions {
    pub(crate) fn from_json(value: Option<serde_json::Value>) -> Self {
        let Some(value) = value.and_then(|value| value.get("indexing").cloned()) else {
            return Self::default();
        };
        match serde_json::from_value(value) {
            Ok(options) => options,
            Err(error) => {
                tracing::warn!(%error, "ignoring invalid workspace indexing configuration");
                Self::default()
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct IndexingCancellation(Arc<AtomicBool>);

impl IndexingCancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceIndexMetrics {
    pub(crate) visited: usize,
    pub(crate) pruned: usize,
    pub(crate) eager: usize,
    pub(crate) resolved: usize,
    pub(crate) unresolved: usize,
    pub(crate) discovery_duration: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceIndexPolicy {
    options: IndexingOptions,
    excludes: Vec<Pattern>,
}

impl Default for WorkspaceIndexPolicy {
    fn default() -> Self {
        Self::new(IndexingOptions::default())
    }
}

impl WorkspaceIndexPolicy {
    pub(crate) fn new(options: IndexingOptions) -> Self {
        let excludes = options.exclude.iter().filter_map(|rule| compile_exclude(rule)).collect();
        Self { options, excludes }
    }

    pub(crate) fn should_prune_directory(
        &self,
        workspace_root: &Path,
        source_root: &Path,
        directory: &Path,
    ) -> bool {
        if !directory.starts_with(source_root) {
            return false;
        }
        if self.excludes_relative_path(workspace_root, directory, true) {
            return true;
        }
        if directory == source_root {
            return false;
        }

        self.excludes_by_name(directory)
            || self.options.exclude_nested_repositories
                && (directory.file_name().is_some_and(|name| name == ".git")
                    || directory.join(".git").exists())
    }

    pub(crate) fn excludes_file(
        &self,
        workspace_root: &Path,
        source_root: &Path,
        path: &Path,
    ) -> bool {
        if !path.starts_with(source_root) {
            return true;
        }
        if self.excludes_relative_path(workspace_root, path, false) {
            return true;
        }

        path.parent()
            .is_some_and(|parent| self.excludes_directory(workspace_root, source_root, parent))
    }

    pub(crate) fn excludes_directory(
        &self,
        workspace_root: &Path,
        source_root: &Path,
        directory: &Path,
    ) -> bool {
        if !directory.starts_with(source_root) {
            return true;
        }
        directory
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(source_root))
            .any(|ancestor| self.should_prune_directory(workspace_root, source_root, ancestor))
    }

    fn excludes_by_name(&self, directory: &Path) -> bool {
        let Some(name) = directory.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        self.options.exclude_hidden_directories && name.starts_with('.')
            || self.options.use_default_excludes && DEFAULT_EXCLUDED_DIRECTORIES.contains(&name)
    }

    fn excludes_relative_path(&self, workspace_root: &Path, path: &Path, directory: bool) -> bool {
        let Some(relative) = portable_relative_path(workspace_root, path) else { return false };
        let options = MatchOptions { require_literal_separator: true, ..MatchOptions::new() };
        self.excludes.iter().any(|pattern| {
            pattern.matches_with(&relative, options)
                || directory && pattern.matches_with(&format!("{relative}/"), options)
        })
    }
}

fn compile_exclude(rule: &str) -> Option<Pattern> {
    let windows_absolute = rule.len() >= 3
        && rule.as_bytes()[0].is_ascii_alphabetic()
        && rule.as_bytes()[1] == b':'
        && matches!(rule.as_bytes()[2], b'/' | b'\\');
    let parent_component = rule.split(['/', '\\']).any(|component| component == "..");
    if rule.starts_with(['/', '\\']) || rule.contains('\\') || windows_absolute || parent_component
    {
        tracing::warn!(rule, "ignoring non-relative workspace indexing exclusion");
        return None;
    }

    match Pattern::new(rule) {
        Ok(pattern) => Some(pattern),
        Err(error) => {
            tracing::warn!(rule, %error, "ignoring invalid workspace indexing exclusion");
            None
        }
    }
}

fn portable_relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut result = String::new();
    for component in relative.components() {
        let Component::Normal(component) = component else { return None };
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(&component.to_string_lossy());
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;

    #[test]
    fn default_hidden_and_custom_exclusions_are_bounded_by_source_root() {
        let project = TestProject::new();
        let workspace_root = project.root();
        let source_root = project.path("/src");
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["src/generated/**".into(), "src/vendor/?old[0-9]/**".into()],
            ..Default::default()
        });

        assert!(policy.should_prune_directory(
            workspace_root,
            &source_root,
            &project.path("/src/node_modules")
        ));
        assert!(policy.should_prune_directory(
            workspace_root,
            &source_root,
            &project.path("/src/.hidden")
        ));
        assert!(policy.should_prune_directory(
            workspace_root,
            &source_root,
            &project.path("/src/generated")
        ));
        assert!(policy.should_prune_directory(
            workspace_root,
            &source_root,
            &project.path("/src/vendor/xold7")
        ));
        assert!(!policy.should_prune_directory(workspace_root, &source_root, &source_root));
        assert!(!policy.excludes_file(
            workspace_root,
            &source_root,
            &project.path("/src/contracts/Token.sol")
        ));
    }

    #[test]
    fn invalid_absolute_and_parent_globs_are_ignored_individually() {
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec![
                "/absolute/**".into(),
                "C:/absolute/**".into(),
                "../escape/**".into(),
                "src/[invalid".into(),
                "src/generated/**".into(),
            ],
            ..Default::default()
        });

        assert_eq!(policy.excludes.len(), 1);
        assert_eq!(policy.excludes[0].as_str(), "src/generated/**");
    }

    #[test]
    fn initialization_options_use_camel_case_indexing_fields() {
        let options = IndexingOptions::from_json(Some(serde_json::json!({
            "indexing": {
                "exclude": ["generated/**"],
                "useDefaultExcludes": false,
                "excludeHiddenDirectories": false,
                "excludeNestedRepositories": false
            }
        })));

        assert_eq!(
            options,
            IndexingOptions {
                exclude: vec!["generated/**".into()],
                use_default_excludes: false,
                exclude_hidden_directories: false,
                exclude_nested_repositories: false,
            }
        );
    }
}
