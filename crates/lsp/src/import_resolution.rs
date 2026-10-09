use crate::{
    document_links::import_path_from_bytes,
    utils::{parse_recovering, span_range},
    workspace::{Workspace, WorkspacePathIndex},
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::source_map::{FileResolver, SourceMap};
use solar_parse::{
    Cursor,
    ast::StrKind,
    lexer::{
        token::{RawLiteralKind, RawTokenKind},
        unescape::try_parse_string_literal,
    },
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_IMPORT_CANDIDATES: usize = 256;

/// The source range and raw contents of an import path at a cursor position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportPathAt {
    pub(crate) raw_path: String,
    pub(crate) content_range: Range<usize>,
    pub(crate) delimiter: u8,
}

/// Finds the parser AST import path containing `cursor` in the current source.
pub(crate) fn import_path_at(source: &str, cursor: usize) -> Option<ImportPathAt> {
    // Import paths are plain strings; code navigation does not need a full-file parse.
    plain_string_at_cursor(source, cursor)?;
    parse_import_path(source, cursor)
}

/// Finds an import path for completion, recovering a plain string left open at `cursor`.
pub(crate) fn import_path_at_for_completion(source: &str, cursor: usize) -> Option<ImportPathAt> {
    // Import paths are plain string tokens. Avoid parsing the whole file for code completions.
    let string = plain_string_at_cursor(source, cursor)?;
    if string.first_unescaped_line_break.is_some_and(|line_break| cursor > line_break) {
        return None;
    }
    if string.terminated && string.first_unescaped_line_break.is_none() {
        return parse_import_path(source, cursor);
    }
    if string.terminated && parse_import_path(source, cursor).is_some() {
        return None;
    }
    recover_unterminated_import_path(source, cursor, string)
}

fn plain_string_at_cursor(source: &str, cursor: usize) -> Option<PlainStringAt> {
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return None;
    }

    // Most navigation requests are issued from ordinary code. Avoid lexing the complete prefix
    // when the cursor's line cannot contain a string (the lexer remains the source of truth when
    // a quote or an escaped line continuation is present).
    if !may_complete_string(source, cursor) {
        return None;
    }
    plain_string_at(source, cursor)
}

/// Rejects code lines that cannot contain a completable import string.
fn may_complete_string(source: &str, cursor: usize) -> bool {
    let prefix = &source.as_bytes()[..cursor];
    let line_break = memchr::memrchr2(b'\r', b'\n', prefix);
    let line_start = line_break.map_or(0, |offset| offset + 1);
    if matches!(source.as_bytes().get(cursor), Some(b'\'' | b'"'))
        || memchr::memchr2(b'\'', b'"', &prefix[line_start..]).is_some()
    {
        return true;
    }
    let Some(mut line_break) = line_break else { return false };
    if prefix[line_break] == b'\n' && line_break > 0 && prefix[line_break - 1] == b'\r' {
        line_break -= 1;
    }
    // A string from an earlier line must cross this line break. Completion already rejects
    // unescaped line breaks; possible continuations still use the full lexer and parser.
    line_break > 0 && prefix[line_break - 1] == b'\\'
}

fn parse_import_path(source: &str, cursor: usize) -> Option<ImportPathAt> {
    let parsed = Arc::new(source.to_owned());
    parse_recovering("lsp-import-resolution.sol", parsed, |_, file, source_unit| {
        let (start, end, raw_path) = source_unit?.imports().find_map(|(_, import)| {
            let range = span_range(file, import.path.span);
            range.contains(&cursor).then(|| (range.start, range.end, import.path.value.as_str()))
        })?;
        let delimiter = *source.as_bytes().get(start)?;
        let content_start = start.checked_add(1)?;
        let content_end = end.checked_sub(1)?;
        if !matches!(delimiter, b'\'' | b'"')
            || source.as_bytes().get(content_end).copied() != Some(delimiter)
        {
            return None;
        }
        let content_range = content_start..content_end;
        source.get(content_range.clone())?;
        Some(ImportPathAt { raw_path: raw_path.to_owned(), content_range, delimiter })
    })
    .flatten()
}

fn recover_unterminated_import_path(
    source: &str,
    cursor: usize,
    string: PlainStringAt,
) -> Option<ImportPathAt> {
    let mut recovered = String::with_capacity(cursor + 2);
    recovered.push_str(&source[..cursor]);
    recovered.push(char::from(string.delimiter));
    recovered.push(';');

    let mut import = parse_import_path(&recovered, cursor)?;
    if import.delimiter != string.delimiter
        || import.content_range != (string.content_range.start..cursor)
    {
        return None;
    }
    import.content_range.end =
        string.first_unescaped_line_break.unwrap_or(string.content_range.end);
    Some(import)
}

struct PlainStringAt {
    content_range: Range<usize>,
    delimiter: u8,
    terminated: bool,
    first_unescaped_line_break: Option<usize>,
}

fn plain_string_at(source: &str, cursor: usize) -> Option<PlainStringAt> {
    for (start, token) in Cursor::new(source).with_position() {
        if start > cursor {
            break;
        }
        let end = start + token.len as usize;
        let RawTokenKind::Literal { kind: RawLiteralKind::Str { kind: StrKind::Str, terminated } } =
            token.kind
        else {
            continue;
        };
        let content_start = start + 1;
        let content_end = if terminated { end - 1 } else { end };
        if !(start..=content_end).contains(&cursor) {
            continue;
        }

        let delimiter = source
            .as_bytes()
            .get(start)
            .copied()
            .filter(|delimiter| matches!(delimiter, b'\'' | b'"'))?;
        let first_unescaped_line_break =
            first_unescaped_line_break(&source.as_bytes()[content_start..content_end])
                .map(|offset| content_start + offset);
        return Some(PlainStringAt {
            content_range: content_start..content_end,
            delimiter,
            terminated,
            first_unescaped_line_break,
        });
    }
    None
}

fn first_unescaped_line_break(bytes: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
            b'\\'
                if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') =>
            {
                index += 3;
            }
            b'\\' if bytes.get(index + 1) == Some(&b'\r') => return Some(index + 1),
            b'\\' => index += 2,
            b'\r' | b'\n' => return Some(index),
            _ => index += 1,
        }
    }
    None
}

pub(crate) fn decode_import_path(path: &str) -> Option<String> {
    String::from_utf8(unescape_import_path(path)?.into_owned()).ok()
}

fn unescape_import_path(path: &str) -> Option<Cow<'_, [u8]>> {
    let mut invalid_escape = false;
    let bytes = try_parse_string_literal(path, StrKind::Str, |_, _| invalid_escape = true);
    (!invalid_escape).then_some(bytes)
}

/// The compiler import configuration owned by one workspace.
#[derive(Clone, Debug)]
pub(crate) struct ImportResolutionContext<'a> {
    workspace_root: PathBuf,
    compile_opts: &'a CompileOpts,
}

impl<'a> ImportResolutionContext<'a> {
    pub(crate) fn for_workspaces_with_index(
        workspaces: &'a [Workspace],
        importing_file: &Path,
        entries: std::sync::Arc<Vec<crate::workspace::WorkspaceImportPathIndexEntry>>,
    ) -> Option<Self> {
        let importing_file = importing_file.normalize();
        let index = WorkspacePathIndex::with_import_entries(workspaces, entries);
        let idx = index.workspace_idx_for_import_path(&importing_file)?;
        let compile_opts = workspaces.get(idx)?.compile_opts();
        let workspace_root = compile_opts.base_path.as_deref()?.normalize();
        Some(Self { workspace_root, compile_opts })
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub(crate) fn compile_opts(&self) -> &'a CompileOpts {
        self.compile_opts
    }
}

pub(crate) struct ImportResolver<'config, 'overlay> {
    context: ImportResolutionContext<'config>,
    overlay_paths: &'overlay [PathBuf],
}

impl<'config, 'overlay> ImportResolver<'config, 'overlay> {
    pub(crate) fn new(
        context: ImportResolutionContext<'config>,
        overlay_paths: &'overlay [PathBuf],
    ) -> Self {
        Self { context, overlay_paths }
    }

    fn file_resolver<'a>(&self, source_map: &'a SourceMap) -> FileResolver<'a> {
        let mut resolver = FileResolver::new(source_map);
        // Relative paths in the options are relative to the workspace.
        resolver.set_current_dir(self.context.workspace_root());
        resolver.configure_from_opts(self.context.compile_opts());
        resolver
    }

    pub(crate) fn complete(&self, importer: &Path, prefix: &str) -> ImportCompletion {
        let source_map = SourceMap::empty();
        let resolver = self.file_resolver(&source_map);

        let (logical_directory, name_prefix) = split_import_prefix(prefix);
        let directory_input = prefix.is_empty() || prefix.ends_with('/');
        let relative_directory_continuation = matches!(name_prefix, "." | "..");
        let mut candidates = BTreeMap::new();
        collect_remapping_candidates(
            &resolver,
            self.context.compile_opts(),
            importer,
            prefix,
            self.overlay_paths,
            &mut candidates,
        );
        let directories = if relative_directory_continuation {
            insert_candidate(
                &mut candidates,
                logical_directory,
                name_prefix,
                ImportCandidateKind::Directory,
            );
            if logical_directory.is_empty() {
                importer.parent().map(Path::to_path_buf).into_iter().collect()
            } else {
                resolver.candidate_paths(Path::new(logical_directory), Some(importer))
            }
        } else {
            resolver
                .candidate_paths(Path::new(prefix), Some(importer))
                .into_iter()
                .filter_map(|path| {
                    if directory_input { Some(path) } else { path.parent().map(Path::to_path_buf) }
                })
                .collect()
        };
        for directory in &directories {
            collect_disk_candidates(directory, logical_directory, name_prefix, &mut candidates);
            collect_overlay_candidates(
                directory,
                logical_directory,
                name_prefix,
                self.overlay_paths,
                &mut candidates,
            );
        }

        let is_incomplete = candidates.len() > MAX_IMPORT_CANDIDATES;
        let candidates = candidates
            .into_iter()
            .take(MAX_IMPORT_CANDIDATES)
            .map(|(import_path, kind)| ImportCandidate { import_path, kind })
            .collect();
        ImportCompletion { candidates, is_incomplete }
    }

    pub(crate) fn resolve(&self, importer: &Path, raw_path: &str) -> Option<PathBuf> {
        let path = import_path_from_bytes(&unescape_import_path(raw_path)?)?;

        let source_map = SourceMap::empty();
        for overlay_path in self.overlay_paths {
            source_map.new_source_file(overlay_path.normalize(), String::new()).ok()?;
        }
        self.file_resolver(&source_map)
            .resolve_file(&path, Some(importer))
            .ok()?
            .name
            .as_real()
            .map(Path::to_path_buf)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportCandidateKind {
    File,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportCandidate {
    import_path: String,
    kind: ImportCandidateKind,
}

impl ImportCandidate {
    pub(crate) fn import_path(&self) -> &str {
        &self.import_path
    }

    pub(crate) fn kind(&self) -> ImportCandidateKind {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportCompletion {
    candidates: Vec<ImportCandidate>,
    is_incomplete: bool,
}

impl ImportCompletion {
    pub(crate) fn candidates(&self) -> &[ImportCandidate] {
        &self.candidates
    }

    pub(crate) fn is_incomplete(&self) -> bool {
        self.is_incomplete
    }
}

fn split_import_prefix(prefix: &str) -> (&str, &str) {
    if prefix.is_empty() || prefix.ends_with('/') {
        return (prefix, "");
    }
    prefix
        .rsplit_once('/')
        .map_or(("", prefix), |(directory, name)| (&prefix[..directory.len() + 1], name))
}

fn collect_remapping_candidates(
    resolver: &FileResolver<'_>,
    opts: &CompileOpts,
    importer: &Path,
    path_prefix: &str,
    overlay_paths: &[PathBuf],
    candidates: &mut BTreeMap<String, ImportCandidateKind>,
) {
    for remapping in &opts.import_remappings {
        let name = remapping.prefix.trim_end_matches('/');
        let directory_candidate = format!("{name}/");
        if ![name, &directory_candidate]
            .into_iter()
            .any(|candidate| candidate != path_prefix && candidate.starts_with(path_prefix))
        {
            continue;
        }

        let remapping_prefix = Path::new(&remapping.prefix);
        let remapped = resolver.remap_import_path(remapping_prefix, Some(importer));
        if remapped.as_ref() != Path::new(&remapping.path) {
            continue;
        }

        let target_paths = resolver.candidate_paths(remapping_prefix, Some(importer));
        let target_is_file = target_paths.iter().any(|target| {
            target.is_file()
                || overlay_paths.iter().any(|path| path.normalize() == target.as_path())
        });
        let target_is_directory = target_paths.iter().any(|target| {
            target.is_dir()
                || overlay_paths
                    .iter()
                    .map(|path| path.normalize())
                    .any(|path| path != *target && path.starts_with(target))
        });
        let kind = if target_is_file
            || (!target_is_directory
                && Path::new(&remapping.path)
                    .extension()
                    .is_some_and(|extension| extension == "sol"))
        {
            ImportCandidateKind::File
        } else {
            ImportCandidateKind::Directory
        };
        let candidate = if kind == ImportCandidateKind::Directory {
            directory_candidate.as_str()
        } else {
            name
        };
        if candidate == path_prefix || !candidate.starts_with(path_prefix) {
            continue;
        }
        insert_candidate(candidates, "", name, kind);
    }
}

fn collect_disk_candidates(
    directory: &Path,
    logical_directory: &str,
    name_prefix: &str,
    candidates: &mut BTreeMap<String, ImportCandidateKind>,
) {
    let Ok(entries) = fs::read_dir(directory) else { return };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| name.starts_with(name_prefix)) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            insert_candidate(candidates, logical_directory, name, ImportCandidateKind::Directory);
        } else if metadata.is_file() && is_import_candidate_file(&entry.path()) {
            insert_candidate(candidates, logical_directory, name, ImportCandidateKind::File);
        }
    }
}

fn collect_overlay_candidates(
    directory: &Path,
    logical_directory: &str,
    name_prefix: &str,
    overlay_paths: &[PathBuf],
    candidates: &mut BTreeMap<String, ImportCandidateKind>,
) {
    for path in overlay_paths {
        let normalized = path.normalize();
        let Ok(relative) = normalized.strip_prefix(directory) else { continue };
        let mut components = relative.components();
        let Some(name) = components.next().and_then(|component| component.as_os_str().to_str())
        else {
            continue;
        };
        if !name.starts_with(name_prefix) {
            continue;
        }
        let kind = if components.next().is_some() {
            ImportCandidateKind::Directory
        } else if is_import_candidate_file(relative) {
            ImportCandidateKind::File
        } else {
            continue;
        };
        insert_candidate(candidates, logical_directory, name, kind);
    }
}

fn is_import_candidate_file(path: &Path) -> bool {
    path.extension().is_none_or(|extension| extension == "sol")
}

fn insert_candidate(
    candidates: &mut BTreeMap<String, ImportCandidateKind>,
    logical_directory: &str,
    name: &str,
    kind: ImportCandidateKind,
) {
    let mut import_path = String::with_capacity(logical_directory.len() + name.len() + 1);
    import_path.push_str(logical_directory);
    import_path.push_str(name);
    if kind == ImportCandidateKind::Directory {
        import_path.push('/');
    }
    candidates.entry(import_path).or_insert(kind);
    if candidates.len() > MAX_IMPORT_CANDIDATES + 1 {
        candidates.pop_last();
    }
}

#[cfg(test)]
#[path = "import_resolution/tests/mod.rs"]
mod tests;
