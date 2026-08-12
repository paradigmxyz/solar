use crate::{
    document_links::import_path_from_bytes,
    workspace::{Workspace, WorkspacePathIndex},
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::{
    Session,
    source_map::{FileName, FileResolver, SourceMap},
};
use solar_parse::{
    Cursor, Parser,
    ast::{self, StrKind},
    lexer::{
        token::{RawLiteralKind, RawTokenKind},
        unescape::try_parse_string_literal,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    ops::Range,
    path::{Path, PathBuf},
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
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return None;
    }

    parse_import_path(source, cursor)
}

/// Finds an import path for completion, recovering a plain string left open at `cursor`.
pub(crate) fn import_path_at_for_completion(source: &str, cursor: usize) -> Option<ImportPathAt> {
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return None;
    }

    let Some(string) = plain_string_at(source, cursor) else {
        return parse_import_path(source, cursor);
    };
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

fn parse_import_path(source: &str, cursor: usize) -> Option<ImportPathAt> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let mut parser = match Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-import-resolution.sol".into()),
            source,
        ) {
            Ok(parser) => parser,
            Err(_) => return None,
        };
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => source_unit,
            Err(error) => {
                error.emit();
                return None;
            }
        };
        drop(parser);

        let files = sess.source_map().files();
        let file = files.first()?;
        let (start, end, raw_path) = source_unit.imports().find_map(|(_, import)| {
            let start = file.relative_position(import.path.span.lo()).to_usize();
            let end = file.relative_position(import.path.span.hi()).to_usize();
            (start..end).contains(&cursor).then(|| (start, end, import.path.value.as_str()))
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
        let end = start + token.len as usize;
        let RawTokenKind::Literal { kind: RawLiteralKind::Str { kind: StrKind::Str, terminated } } =
            token.kind
        else {
            continue;
        };
        let content_start = start + 1;
        let content_end = if terminated { end - 1 } else { end };
        if !(content_start..=content_end).contains(&cursor) {
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
    let mut invalid_escape = false;
    let bytes = try_parse_string_literal(path, StrKind::Str, |_, _| {
        invalid_escape = true;
    });
    if invalid_escape {
        return None;
    }
    String::from_utf8(bytes.into_owned()).ok()
}

/// The compiler import configuration owned by one workspace.
#[derive(Clone, Debug)]
pub(crate) struct ImportResolutionContext<'a> {
    workspace_root: PathBuf,
    compile_opts: &'a CompileOpts,
}

impl<'a> ImportResolutionContext<'a> {
    pub(crate) fn for_workspaces(
        workspaces: &'a [Workspace],
        importing_file: &Path,
    ) -> Option<Self> {
        let importing_file = importing_file.normalize();
        let idx =
            WorkspacePathIndex::new(workspaces).workspace_idx_for_import_path(&importing_file)?;
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

    pub(crate) fn complete(&self, importer: &Path, prefix: &str) -> ImportCompletion {
        let source_map = SourceMap::empty();
        let mut resolver = FileResolver::new(&source_map);
        resolver.configure_from_opts(self.context.compile_opts());
        resolver.set_current_dir(self.context.workspace_root());

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
        let mut invalid_escape = false;
        let path = try_parse_string_literal(raw_path, StrKind::Str, |_, _| {
            invalid_escape = true;
        });
        if invalid_escape {
            return None;
        }
        let path = import_path_from_bytes(path.as_ref())?;

        let source_map = SourceMap::empty();
        for overlay_path in self.overlay_paths {
            source_map.new_source_file(overlay_path.normalize(), String::new()).ok()?;
        }
        let mut resolver = FileResolver::new(&source_map);
        resolver.configure_from_opts(self.context.compile_opts());
        resolver.set_current_dir(self.context.workspace_root());

        resolver.resolve_file(&path, Some(importer)).ok()?.name.as_real().map(Path::to_path_buf)
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
        let mut candidate = name.to_owned();
        if kind == ImportCandidateKind::Directory {
            candidate.push('/');
        }
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
