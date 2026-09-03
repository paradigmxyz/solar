//! Manifest-backed, process-isolated Solidity fixtures.

use crate::{
    config::{CompilerSpec, FixtureSpec, SourceSpec},
    lifecycle::{VERSION_PROBE_TIMEOUT, git_output, inspect_compiler_version},
};
use anyhow::{Context, Result, bail};
use lsp_types::{Position, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const IGNORED_DIRECTORIES: [&str; 7] =
    [".git", "out", "cache", "broadcast", "node_modules", "target", ".vscode"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureMetadata {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
    pub(crate) revision: Option<String>,
    pub(crate) source_file_count: usize,
    pub(crate) source_line_count: usize,
    pub(crate) source_byte_count: usize,
    pub(crate) content_sha256: String,
    pub(crate) corpus: Option<String>,
    pub(crate) source: Option<SourceSpec>,
    pub(crate) solc: Option<CompilerSpec>,
    pub(crate) solc_native_sha256: Option<String>,
    pub(crate) solc_soljson_sha256: Option<String>,
    #[serde(default)]
    pub(crate) solc_native_version: Option<String>,
    pub(crate) foundry: Option<CompilerSpec>,
    pub(crate) foundry_native_sha256: Option<String>,
    #[serde(default)]
    pub(crate) foundry_native_version: Option<String>,
    pub(crate) dependencies: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct FixtureSource {
    spec: FixtureSpec,
    root: PathBuf,
    metadata: FixtureMetadata,
}

pub(crate) struct Fixture {
    root: TempDir,
    source: FixtureSource,
}

#[derive(Clone, Debug)]
pub(crate) struct Anchor {
    pub(crate) path: PathBuf,
    pub(crate) position: Position,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

impl PositionEncoding {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "utf-8" => Ok(Self::Utf8),
            "utf-16" => Ok(Self::Utf16),
            "utf-32" => Ok(Self::Utf32),
            _ => bail!("unsupported position encoding `{value}`"),
        }
    }
}

impl FixtureSource {
    pub(crate) fn open(spec: &FixtureSpec) -> Result<Self> {
        let root = spec
            .root
            .canonicalize()
            .with_context(|| format!("fixture `{}` does not exist", spec.root.display()))?;
        if !root.is_dir() {
            bail!("fixture root `{}` is not a directory", root.display())
        }
        if let Some(expected) = &spec.revision {
            validate_git_state(&root, expected)?;
        }

        for source_root in &spec.source_roots {
            if let Some(ignored) = ignored_directory(source_root) {
                bail!(
                    "fixture `{}` source root `{}` contains ignored directory `{ignored}`",
                    spec.id,
                    source_root.display()
                )
            }
        }
        let source_roots = spec.source_roots.iter().map(|path| root.join(path)).collect::<Vec<_>>();
        let mut paths = Vec::new();
        for source_root in &source_roots {
            if !source_root.is_dir() {
                bail!(
                    "fixture `{}` source root `{}` is not a directory",
                    spec.id,
                    source_root.display()
                )
            }
            collect_solidity_files(&root, source_root, &mut paths)?;
        }
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            bail!("fixture `{}` contains no Solidity sources", spec.id)
        }

        let mut lines = 0;
        let mut bytes = 0;
        for path in &paths {
            let contents = fs::read(path)
                .with_context(|| format!("failed to read fixture source `{}`", path.display()))?;
            lines += contents.iter().filter(|byte| **byte == b'\n').count();
            bytes += contents.len();
        }

        let solc_native_sha256 = compiler_file_sha256(
            spec.solc.as_ref().and_then(|compiler| compiler.native.as_deref()),
        );
        let solc_soljson_sha256 = compiler_file_sha256(
            spec.solc.as_ref().and_then(|compiler| compiler.soljson.as_deref()),
        );
        let foundry_native_sha256 = compiler_file_sha256(
            spec.foundry.as_ref().and_then(|compiler| compiler.native.as_deref()),
        );
        if let Some(solc) = &spec.solc {
            verify_compiler_file_digest(
                "solc native",
                solc.native_sha256.as_deref(),
                solc_native_sha256.as_deref(),
            )?;
            verify_compiler_file_digest(
                "solc soljson",
                solc.soljson_sha256.as_deref(),
                solc_soljson_sha256.as_deref(),
            )?;
        }
        if let Some(foundry) = &spec.foundry {
            verify_compiler_file_digest(
                "foundry native",
                foundry.native_sha256.as_deref(),
                foundry_native_sha256.as_deref(),
            )?;
        }
        let solc_native_version = spec
            .solc
            .as_ref()
            .map(|compiler| inspect_compiler_version("solc", compiler, VERSION_PROBE_TIMEOUT))
            .transpose()?
            .flatten();
        let foundry_native_version = spec
            .foundry
            .as_ref()
            .map(|compiler| inspect_compiler_version("foundry", compiler, VERSION_PROBE_TIMEOUT))
            .transpose()?
            .flatten();

        let revision = spec.revision.clone().or_else(|| git_revision(&root));
        let content_sha256 = fixture_content_sha256(&root)?;
        let metadata = FixtureMetadata {
            id: spec.id.clone(),
            root: root.clone(),
            revision,
            source_file_count: paths.len(),
            source_line_count: lines,
            source_byte_count: bytes,
            content_sha256,
            corpus: spec.corpus.clone(),
            source: spec.source.clone(),
            solc: spec.solc.clone(),
            solc_native_sha256,
            solc_soljson_sha256,
            solc_native_version,
            foundry: spec.foundry.clone(),
            foundry_native_sha256,
            foundry_native_version,
            dependencies: spec.dependencies.clone(),
        };
        let source = Self { spec: spec.clone(), root, metadata };
        source.validate_anchors()?;
        Ok(source)
    }

    pub(crate) fn metadata(&self) -> &FixtureMetadata {
        &self.metadata
    }

    pub(crate) fn materialize(&self) -> Result<Fixture> {
        let destination = tempfile::tempdir()?;
        copy_tree(&self.root, &self.root, destination.path())?;
        Ok(Fixture { root: destination, source: self.clone() })
    }

    fn validate_anchors(&self) -> Result<()> {
        for (name, anchor) in &self.spec.anchors {
            let path = fixture_path(&self.root, &anchor.path)?;
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read anchor file `{}`", path.display()))?;
            let matches = text.match_indices(&anchor.needle).count();
            if matches != 1 {
                bail!(
                    "fixture `{}` anchor `{name}` must match exactly once in `{}`; found {matches}",
                    self.spec.id,
                    anchor.path.display()
                )
            }
            if anchor.offset > anchor.needle.len() {
                bail!(
                    "fixture `{}` anchor `{name}` offset {} exceeds needle length {}",
                    self.spec.id,
                    anchor.offset,
                    anchor.needle.len()
                )
            }
            let _ = position_at(&text, text.find(&anchor.needle).unwrap() + anchor.offset);
        }
        Ok(())
    }
}

impl Fixture {
    pub(crate) fn root(&self) -> &Path {
        self.root.path()
    }

    pub(crate) fn metadata(&self) -> &FixtureMetadata {
        self.source.metadata()
    }

    pub(crate) fn source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for source_root in &self.source.spec.source_roots {
            collect_solidity_files(
                self.root.path(),
                &self.root.path().join(source_root),
                &mut files,
            )?;
        }
        files.sort();
        files.dedup();
        files
            .into_iter()
            .map(|path| path.strip_prefix(self.root()).map(PathBuf::from))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("fixture source file escaped its materialized root")
    }

    pub(crate) fn path(&self, relative: &Path) -> Result<PathBuf> {
        fixture_path(self.root(), relative)
    }

    pub(crate) fn anchor(&self, name: &str) -> Result<Anchor> {
        self.anchor_with_encoding(name, PositionEncoding::Utf16)
    }

    pub(crate) fn anchor_with_encoding(
        &self,
        name: &str,
        encoding: PositionEncoding,
    ) -> Result<Anchor> {
        let spec =
            self.source.spec.anchors.get(name).with_context(|| {
                format!("fixture `{}` has no anchor `{name}`", self.metadata().id)
            })?;
        let path = self.path(&spec.path)?;
        let text = fs::read_to_string(&path)?;
        self.anchor_in_text(name, &path, &text, encoding)
    }

    pub(crate) fn anchor_in_text(
        &self,
        name: &str,
        path: &Path,
        text: &str,
        encoding: PositionEncoding,
    ) -> Result<Anchor> {
        let spec =
            self.source.spec.anchors.get(name).with_context(|| {
                format!("fixture `{}` has no anchor `{name}`", self.metadata().id)
            })?;
        let expected_path = self.path(&spec.path)?;
        if expected_path != path {
            bail!("anchor `{name}` belongs to `{}`, not `{}`", spec.path.display(), path.display())
        }
        let offset = text.find(&spec.needle).with_context(|| {
            format!("anchor `{name}` disappeared from `{}`", spec.path.display())
        })? + spec.offset;
        Ok(Anchor {
            path: path.to_owned(),
            position: position_at_with_encoding(text, offset, encoding),
        })
    }

    pub(crate) fn anchor_needle(&self, name: &str) -> Result<String> {
        self.source
            .spec
            .anchors
            .get(name)
            .map(|anchor| anchor.needle.clone())
            .with_context(|| format!("fixture `{}` has no anchor `{name}`", self.metadata().id))
    }
}

fn fixture_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("fixture path `{}` must be relative and stay in its root", relative.display())
    }
    let path = root.join(relative);
    if !path.starts_with(root) {
        bail!("fixture path `{}` escapes its root", relative.display())
    }
    Ok(path)
}

pub(crate) fn file_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path)
        .map_err(|()| anyhow::anyhow!("invalid fixture file path `{}`", path.display()))
}

pub(crate) fn position_at(text: &str, offset: usize) -> Position {
    position_at_with_encoding(text, offset, PositionEncoding::Utf16)
}

pub(crate) fn position_at_with_encoding(
    text: &str,
    offset: usize,
    encoding: PositionEncoding,
) -> Position {
    let prefix = &text[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line_prefix = &prefix[line_start..];
    let character = match encoding {
        PositionEncoding::Utf8 => line_prefix.len(),
        PositionEncoding::Utf16 => line_prefix.encode_utf16().count(),
        PositionEncoding::Utf32 => line_prefix.chars().count(),
    };
    Position { line, character: character as u32 }
}

pub(crate) fn offset_at_position(
    text: &str,
    position: Position,
    encoding: PositionEncoding,
) -> Result<usize> {
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(relative) = text[line_start..].find('\n') else {
            bail!("position line {} is outside the document", position.line)
        };
        line_start += relative + 1;
    }
    let line_end = text[line_start..].find('\n').map_or(text.len(), |index| line_start + index);
    let line = &text[line_start..line_end];
    let target = position.character as usize;
    if target == 0 {
        return Ok(line_start);
    }
    let mut units = 0;
    for (offset, character) in line.char_indices() {
        units += match encoding {
            PositionEncoding::Utf8 => character.len_utf8(),
            PositionEncoding::Utf16 => character.len_utf16(),
            PositionEncoding::Utf32 => 1,
        };
        if units == target {
            return Ok(line_start + offset + character.len_utf8());
        }
        if units > target {
            bail!("position character {} splits a Unicode scalar", position.character)
        }
    }
    bail!("position character {} is outside line {}", position.character, position.line)
}

fn validate_git_state(root: &Path, expected: &str) -> Result<()> {
    let revision = git_output(root, &["rev-parse", "HEAD"])?;
    if revision != expected {
        bail!("fixture `{}` must be at `{expected}`, found `{revision}`", root.display())
    }
    let status = git_output(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !status.is_empty() {
        bail!("fixture `{}` has a dirty working tree", root.display())
    }
    Ok(())
}

fn git_revision(root: &Path) -> Option<String> {
    git_output(root, &["rev-parse", "HEAD"]).ok()
}

fn collect_solidity_files(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            if entry.file_name().to_str().is_some_and(|name| IGNORED_DIRECTORIES.contains(&name)) {
                continue;
            }
            collect_solidity_files(root, &path, paths)?;
        } else if file_type.is_file() && path.extension() == Some(OsStr::new("sol")) {
            paths.push(path);
        } else if file_type.is_symlink() {
            symlink_file_target(root, &path)?;
            if path.extension() == Some(OsStr::new("sol")) {
                paths.push(path);
            }
        }
    }
    Ok(())
}

fn ignored_directory(path: &Path) -> Option<&str> {
    path.components().find_map(|component| {
        let name = component.as_os_str().to_str()?;
        IGNORED_DIRECTORIES.contains(&name).then_some(name)
    })
}

fn fixture_content_sha256(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_fixture_files(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        let contents = fs::read(root.join(&relative))?;
        hasher.update(contents.len().to_le_bytes());
        hasher.update(contents);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_fixture_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            if entry.file_name().to_str().is_some_and(|name| IGNORED_DIRECTORIES.contains(&name)) {
                continue;
            }
            collect_fixture_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root)?.to_path_buf());
        } else if file_type.is_symlink() {
            symlink_file_target(root, &path)?;
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn compiler_file_sha256(path: Option<&Path>) -> Option<String> {
    let path = path.filter(|path| path.is_file())?;
    let contents = fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(contents);
    Some(format!("{:x}", hasher.finalize()))
}

fn verify_compiler_file_digest(
    kind: &str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Result<()> {
    let Some(expected) = expected else { return Ok(()) };
    let Some(actual) = actual else { bail!("{kind} artifact digest is unavailable") };
    if !expected.eq_ignore_ascii_case(actual) {
        bail!("{kind} artifact digest mismatch: expected {expected}, found {actual}")
    }
    Ok(())
}

fn copy_tree(root: &Path, source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| IGNORED_DIRECTORIES.contains(&name)) {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_tree(root, &source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        } else if file_type.is_symlink() {
            fs::copy(symlink_file_target(root, &source_path)?, &destination_path)?;
        }
    }
    Ok(())
}

fn symlink_file_target(root: &Path, path: &Path) -> Result<PathBuf> {
    let target = path
        .canonicalize()
        .with_context(|| format!("failed to resolve fixture symlink `{}`", path.display()))?;
    if !target.starts_with(root) {
        bail!("fixture symlink `{}` escapes the fixture root", path.display())
    }
    if !target.is_file() {
        bail!("fixture symlink `{}` does not target a regular file", path.display())
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AnchorSpec, FixtureSpec};
    use std::collections::BTreeMap;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn older_schema_five_metadata_defaults_observed_compiler_versions() {
        let metadata = serde_json::from_value::<FixtureMetadata>(serde_json::json!({
            "id": "fixture",
            "root": "/fixture",
            "revision": null,
            "source_file_count": 1,
            "source_line_count": 1,
            "source_byte_count": 1,
            "content_sha256": "a",
            "corpus": null,
            "source": null,
            "solc": null,
            "solc_native_sha256": null,
            "solc_soljson_sha256": null,
            "foundry": null,
            "foundry_native_sha256": null,
            "dependencies": {}
        }))
        .unwrap();

        assert!(metadata.solc_native_version.is_none());
        assert!(metadata.foundry_native_version.is_none());
    }

    #[test]
    fn validates_shape_and_resolves_utf16_anchor() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(root.path().join("src/Main.sol"), "// 😀\ncontract Main { uint x; }\n").unwrap();
        let spec = FixtureSpec {
            id: "fixture".into(),
            root: root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec!["src".into()],
            anchors: BTreeMap::from([(
                "x".into(),
                AnchorSpec { path: "src/Main.sol".into(), needle: "x".into(), offset: 0 },
            )]),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let source = FixtureSource::open(&spec).unwrap();
        assert_eq!(source.metadata().source_file_count, 1);
        let fixture = source.materialize().unwrap();
        let anchor = fixture.anchor("x").unwrap();
        assert_eq!(anchor.position, Position { line: 1, character: 21 });
    }

    #[test]
    fn rejects_duplicate_anchor_text() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Main.sol"), "x x").unwrap();
        let spec = FixtureSpec {
            id: "fixture".into(),
            root: root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::from([(
                "x".into(),
                AnchorSpec { path: "Main.sol".into(), needle: "x".into(), offset: 0 },
            )]),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        assert!(FixtureSource::open(&spec).is_err());
    }

    #[test]
    fn rejects_explicitly_ignored_source_roots() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("cache")).unwrap();
        fs::write(root.path().join("cache/Main.sol"), "contract Main {}\n").unwrap();
        let mut spec = fixture_spec(root.path());
        spec.source_roots = vec!["cache".into()];

        let error = FixtureSource::open(&spec).unwrap_err().to_string();

        assert!(error.contains("ignored directory `cache`"), "{error}");
    }

    #[test]
    fn rejects_mismatched_fixture_compiler_artifacts() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let compiler = root.path().join("solc");
        fs::write(&compiler, "not the pinned compiler").unwrap();
        let spec = FixtureSpec {
            id: "fixture".into(),
            root: root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: Some(CompilerSpec {
                version: "1.0".into(),
                native: Some(compiler),
                soljson: None,
                native_url: Some("https://example.invalid/solc".into()),
                native_sha256: Some("0".repeat(64)),
                soljson_url: None,
                soljson_sha256: None,
                archive_url: None,
                archive_sha256: None,
            }),
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };

        let error = FixtureSource::open(&spec).unwrap_err().to_string();

        assert!(error.contains("solc native artifact digest mismatch"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn materializes_internal_file_symlinks_as_regular_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        fs::write(root.path().join("AGENTS.md"), "fixture instructions\n").unwrap();
        symlink("AGENTS.md", root.path().join("CLAUDE.md")).unwrap();

        let source = FixtureSource::open(&fixture_spec(root.path())).unwrap();
        let fixture = source.materialize().unwrap();
        let copied = fixture.root().join("CLAUDE.md");

        assert_eq!(fs::read_to_string(&copied).unwrap(), "fixture instructions\n");
        assert!(fs::symlink_metadata(copied).unwrap().file_type().is_file());
    }

    #[cfg(unix)]
    #[test]
    fn includes_internal_solidity_file_symlinks_in_source_inventory() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Real.sol"), "contract Real {}\n").unwrap();
        symlink("Real.sol", root.path().join("Alias.sol")).unwrap();

        let source = FixtureSource::open(&fixture_spec(root.path())).unwrap();
        assert_eq!(source.metadata().source_file_count, 2);

        let fixture = source.materialize().unwrap();
        assert_eq!(
            fixture.source_files().unwrap(),
            vec![PathBuf::from("Alias.sol"), PathBuf::from("Real.sol")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_escape_the_fixture_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("fixture");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Main.sol"), "contract Main {}\n").unwrap();
        fs::write(directory.path().join("outside"), "runner data\n").unwrap();
        symlink("../outside", root.join("LEAK")).unwrap();

        let error = FixtureSource::open(&fixture_spec(&root)).unwrap_err().to_string();

        assert!(error.contains("escapes the fixture root"), "{error}");
    }

    #[test]
    fn positions_round_trip_for_all_lsp_encodings() {
        let text = "prefix\na😀éz";
        let offset = text.find('é').unwrap();
        let expectations = [
            (PositionEncoding::Utf8, 5),
            (PositionEncoding::Utf16, 3),
            (PositionEncoding::Utf32, 2),
        ];
        for (encoding, character) in expectations {
            let position = position_at_with_encoding(text, offset, encoding);
            assert_eq!(position, Position { line: 1, character });
            assert_eq!(offset_at_position(text, position, encoding).unwrap(), offset);
        }
    }

    #[test]
    fn utf16_position_rejects_half_surrogate_offsets() {
        let error =
            offset_at_position("😀", Position { line: 0, character: 1 }, PositionEncoding::Utf16)
                .unwrap_err();
        assert!(error.to_string().contains("splits a Unicode scalar"));
    }

    fn fixture_spec(root: &Path) -> FixtureSpec {
        FixtureSpec {
            id: "fixture".into(),
            root: root.into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        }
    }
}
