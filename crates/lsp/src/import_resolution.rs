use crate::{
    document_links::{import_path_from_bytes, solidity_string_contents},
    workspace::{Workspace, WorkspacePathIndex},
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::source_map::{FileResolver, SourceMap};
use solar_parse::{
    Cursor,
    ast::{
        StrKind,
        token::{BinOpToken, Delimiter},
    },
    lexer::{
        token::{RawLiteralKind, RawTokenKind},
        unescape::try_parse_string_literal,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    ops::Range as ByteRange,
    path::{Path, PathBuf},
};

const MAX_IMPORT_CANDIDATES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportCursor<'a> {
    path_prefix: &'a str,
    complete_path: Option<&'a str>,
    replacement_range: ByteRange<usize>,
    delimiter: u8,
}

impl<'a> ImportCursor<'a> {
    pub(crate) fn at(source: &'a str, cursor: usize) -> Option<Self> {
        if cursor > source.len() || !source.is_char_boundary(cursor) {
            return None;
        }

        let mut brace_depth = 0usize;
        let mut import_tokens = None::<Vec<SyntaxToken>>;
        for (start, token) in Cursor::new(source).with_position() {
            if start > cursor {
                break;
            }
            let end = start + token.len as usize;
            if token.kind.is_trivial() {
                continue;
            }

            if let Some(tokens) = &mut import_tokens {
                if token.kind == RawTokenKind::Ident && source[start..end] == *"import" {
                    tokens.clear();
                    continue;
                }
                if token.kind == RawTokenKind::Semi {
                    import_tokens = None;
                    continue;
                }
                if let RawTokenKind::Literal {
                    kind: RawLiteralKind::Str { kind: StrKind::Str, terminated },
                } = token.kind
                    && import_path_follows(source, tokens)
                {
                    let content_start = start + 1;
                    let raw_content_end = if terminated { end - 1 } else { end };
                    if (start..=raw_content_end).contains(&cursor) {
                        let prefix_end = cursor.max(content_start);
                        let path_prefix = &source[content_start..prefix_end];
                        // The raw lexer permits newlines in strings and may consume a quote on a
                        // later line. Only escaped LF/CRLF continuations are legal Solidity string
                        // contents, so an unescaped line break keeps recovery edits local.
                        let terminated = terminated
                            && !has_unescaped_line_break(
                                &source.as_bytes()[content_start..raw_content_end],
                            );
                        let content_end =
                            if terminated { end - 1 } else { cursor.max(content_start) };
                        return Some(Self {
                            path_prefix,
                            complete_path: terminated.then(|| &source[content_start..content_end]),
                            replacement_range: content_start..content_end,
                            delimiter: source.as_bytes()[start],
                        });
                    }
                }
                tokens.push(SyntaxToken { kind: token.kind, start, end });
                continue;
            }

            if brace_depth == 0
                && token.kind == RawTokenKind::Ident
                && source[start..end] == *"import"
            {
                import_tokens = Some(Vec::new());
                continue;
            }
            match token.kind {
                RawTokenKind::OpenDelim(Delimiter::Brace) => brace_depth += 1,
                RawTokenKind::CloseDelim(Delimiter::Brace) => {
                    brace_depth = brace_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn decoded_path_prefix(&self) -> Option<String> {
        let mut invalid_escape = false;
        let bytes = try_parse_string_literal(self.path_prefix, StrKind::Str, |_, _| {
            invalid_escape = true;
        });
        if invalid_escape {
            return None;
        }
        String::from_utf8(bytes.into_owned()).ok()
    }

    pub(crate) fn escaped_path(&self, path: &str) -> String {
        solidity_string_contents(path.as_bytes(), self.delimiter)
    }

    pub(crate) fn filter_text(&self, path: &str, decoded_prefix: &str) -> String {
        if self.path_prefix.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return self.escaped_path(path);
        }
        let Some(suffix) = path.strip_prefix(decoded_prefix) else {
            return self.escaped_path(path);
        };
        let mut filter_text = String::with_capacity(self.path_prefix.len() + suffix.len());
        filter_text.push_str(self.path_prefix);
        filter_text.push_str(&self.escaped_path(suffix));
        filter_text
    }

    pub(crate) fn complete_path(&self) -> Option<&'a str> {
        self.complete_path
    }

    pub(crate) fn replacement_range(&self) -> ByteRange<usize> {
        self.replacement_range.clone()
    }
}

fn has_unescaped_line_break(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
            b'\\'
                if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') =>
            {
                index += 3;
            }
            b'\\' if bytes.get(index + 1) == Some(&b'\r') => return true,
            b'\\' => index += 2,
            b'\r' | b'\n' => return true,
            _ => index += 1,
        }
    }
    false
}

#[derive(Clone, Copy, Debug)]
struct SyntaxToken {
    kind: RawTokenKind,
    start: usize,
    end: usize,
}

fn import_path_follows(source: &str, tokens: &[SyntaxToken]) -> bool {
    if tokens.is_empty() {
        return true;
    }
    if let [star, as_kw, alias, from_kw] = tokens
        && star.kind == RawTokenKind::BinOp(BinOpToken::Star)
    {
        return token_is_ident(source, as_kw, "as")
            && alias.kind == RawTokenKind::Ident
            && token_is_ident(source, from_kw, "from");
    }
    if tokens.len() >= 3
        && tokens[0].kind == RawTokenKind::OpenDelim(Delimiter::Brace)
        && tokens[tokens.len() - 2].kind == RawTokenKind::CloseDelim(Delimiter::Brace)
        && token_is_ident(source, &tokens[tokens.len() - 1], "from")
    {
        let mut depth = 0usize;
        for token in &tokens[..tokens.len() - 1] {
            match token.kind {
                RawTokenKind::OpenDelim(Delimiter::Brace) => depth += 1,
                RawTokenKind::CloseDelim(Delimiter::Brace) => {
                    let Some(next) = depth.checked_sub(1) else { return false };
                    depth = next;
                }
                _ => {}
            }
        }
        return depth == 0;
    }
    false
}

fn token_is_ident(source: &str, token: &SyntaxToken, expected: &str) -> bool {
    token.kind == RawTokenKind::Ident && source[token.start..token.end] == *expected
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
