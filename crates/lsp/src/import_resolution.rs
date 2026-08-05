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

    #[cfg(test)]
    pub(crate) fn path_prefix(&self) -> &'a str {
        self.path_prefix
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
mod tests {
    use super::{ImportCursor, ImportResolutionContext, ImportResolver, MAX_IMPORT_CANDIDATES};
    use crate::{test_support::TestProject, workspace::Workspace};

    fn marked_source(source: &str) -> (String, usize) {
        let cursor = source.find("$0").unwrap();
        (source.replacen("$0", "", 1), cursor)
    }

    #[test]
    fn import_cursor_recovers_an_unterminated_path_without_consuming_the_next_line() {
        let (source, offset) = marked_source("import \"./Dep$0\ncontract C {}\n");

        let cursor = ImportCursor::at(&source, offset).unwrap();

        assert_eq!(cursor.path_prefix(), "./Dep");
        assert_eq!(&source[cursor.replacement_range()], "./Dep");
        assert_eq!(cursor.complete_path(), None);
    }

    #[test]
    fn import_cursor_does_not_use_a_quote_on_the_next_line_as_the_terminator() {
        let (source, offset) =
            marked_source("import \"./Dep$0\ncontract C { string value = \"ordinary\"; }\n");

        let cursor = ImportCursor::at(&source, offset).unwrap();

        assert_eq!(cursor.path_prefix(), "./Dep");
        assert_eq!(&source[cursor.replacement_range()], "./Dep");
        assert_eq!(cursor.complete_path(), None);
    }

    #[test]
    fn import_cursor_does_not_replace_unknown_unterminated_suffixes() {
        let (source, offset) = marked_source("import \"./Dep$0 contract C {}");

        let cursor = ImportCursor::at(&source, offset).unwrap();

        assert_eq!(cursor.path_prefix(), "./Dep");
        assert_eq!(&source[cursor.replacement_range()], "./Dep");
        assert_eq!(cursor.complete_path(), None);
    }

    #[test]
    fn import_cursor_supports_escaped_line_continuations() {
        for newline in ["\n", "\r\n"] {
            let marked = format!("import \"./nested/\\{newline}    Tar$0get.sol\";");
            let (source, offset) = marked_source(&marked);

            let cursor = ImportCursor::at(&source, offset).unwrap();

            assert_eq!(cursor.decoded_path_prefix().as_deref(), Some("./nested/Tar"));
            assert_eq!(
                &source[cursor.replacement_range()],
                format!("./nested/\\{newline}    Target.sol")
            );
            assert_eq!(
                cursor.complete_path(),
                Some(format!("./nested/\\{newline}    Target.sol").as_str())
            );
        }
    }

    #[test]
    fn import_cursor_recovers_after_an_import_missing_its_semicolon() {
        let (source, offset) = marked_source("import \"./First.sol\"\nimport \"./Second$0.sol\";");

        let cursor = ImportCursor::at(&source, offset).unwrap();

        assert_eq!(cursor.path_prefix(), "./Second");
        assert_eq!(&source[cursor.replacement_range()], "./Second.sol");
        assert_eq!(cursor.complete_path(), Some("./Second.sol"));
    }

    #[test]
    fn import_cursor_recognizes_import_forms_and_rejects_ordinary_strings() {
        for marked in [
            "import \"./Dep$0\";",
            "import \"./Dep$0\" as Dependency;",
            "import * as Dependency from \"./Dep$0\";",
            "import {A, B as C} from \"./Dep$0\";",
            "import {\n    A\n} from './Dep$0';",
        ] {
            let (source, offset) = marked_source(marked);
            let cursor = ImportCursor::at(&source, offset)
                .unwrap_or_else(|| panic!("expected an import cursor for {source:?}"));

            assert_eq!(cursor.path_prefix(), "./Dep");
            assert_eq!(&source[cursor.replacement_range()], "./Dep");
            assert_eq!(cursor.complete_path(), Some("./Dep"));
        }

        for marked in [
            "string constant VALUE = \"./Dep$0\";",
            "contract C { function f() external { string memory x = \"./Dep$0\"; } }",
        ] {
            let (source, offset) = marked_source(marked);
            assert_eq!(ImportCursor::at(&source, offset), None, "source: {source:?}");
        }
    }

    #[test]
    fn resolver_completes_only_one_relative_directory_level_and_includes_overlay_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /src/Main.sol
            import "./";

            //- /src/Local.sol
            contract Local {}

            //- /src/nested/OnDisk.sol
            contract OnDisk {}

            //- /src/README.md
            not Solidity
            "#,
        );
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let context = config.import_resolution_context(&importer).unwrap();
        let overlay =
            [project.path("/src/Unsaved.sol"), project.path("/src/virtual/OnlyInOverlay.sol")];

        let completion = ImportResolver::new(context, &overlay).complete(&importer, "./");
        let candidates = completion
            .candidates()
            .iter()
            .map(|candidate| candidate.import_path())
            .collect::<Vec<_>>();

        assert_eq!(
            candidates,
            vec!["./Local.sol", "./Main.sol", "./Unsaved.sol", "./nested/", "./virtual/"]
        );
        assert!(!completion.is_incomplete());
    }

    #[test]
    fn resolver_completes_bare_relative_directory_segments() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /Root.sol
            contract Root {}

            //- /src/Main.sol
            import ".";

            //- /src/Local.sol
            contract Local {}
            "#,
        );
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let context = config.import_resolution_context(&importer).unwrap();
        let resolver = ImportResolver::new(context, &[]);

        for (prefix, expected) in [(".", "./"), ("..", "../"), ("./.", "././"), ("../..", "../../")]
        {
            let completion = resolver.complete(&importer, prefix);
            assert_eq!(
                completion
                    .candidates()
                    .iter()
                    .map(|candidate| candidate.import_path())
                    .collect::<Vec<_>>(),
                [expected],
                "prefix: {prefix:?}"
            );
        }
    }

    #[test]
    fn resolver_keeps_dotfiles_beside_relative_directory_continuations() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /src/Main.sol
            import ".";

            //- /src/.b.sol
            contract SingleDot {}

            //- /src/..b.sol
            contract DoubleDot {}
            "#,
        );
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let context = config.import_resolution_context(&importer).unwrap();
        let resolver = ImportResolver::new(context, &[]);

        for (prefix, expected) in
            [(".", vec!["..b.sol", "./", ".b.sol"]), ("..", vec!["../", "..b.sol"])]
        {
            let completion = resolver.complete(&importer, prefix);
            assert_eq!(
                completion
                    .candidates()
                    .iter()
                    .map(|candidate| candidate.import_path())
                    .collect::<Vec<_>>(),
                expected,
                "prefix: {prefix:?}"
            );
        }
    }

    #[test]
    fn resolver_resolves_an_overlay_only_import() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /src/Main.sol
            import "./Unsaved.sol";
            "#,
        );
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let target = project.path("/src/Unsaved.sol");
        let context = config.import_resolution_context(&importer).unwrap();
        let overlay = [target.clone()];

        let resolved = ImportResolver::new(context, &overlay).resolve(&importer, "./Unsaved.sol");

        assert_eq!(resolved, Some(target));
    }

    #[test]
    fn resolver_rejects_ambiguous_exact_imports() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            auto_detect_remappings = false
            libs = ["lib", "vendor"]

            //- /src/Main.sol
            import "pkg/Target.sol";

            //- /lib/pkg/Target.sol
            contract LibTarget {}

            //- /vendor/pkg/Target.sol
            contract VendorTarget {}
            "#,
        );
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let context = config.import_resolution_context(&importer).unwrap();

        let resolved = ImportResolver::new(context, &[]).resolve(&importer, "pkg/Target.sol");

        assert_eq!(resolved, None);
    }

    #[test]
    fn resolver_normalizes_the_foundry_root_before_applying_context_remappings() {
        let project = TestProject::from_fixture(
            r#"
            //- /container/.keep

            //- /project/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["src:pkg/=lib/"]

            //- /project/src/Main.sol
            import "pkg/Target.sol";

            //- /project/lib/Target.sol
            contract Target {}
            "#,
        );
        let manifest = project.path("/container/../project/foundry.toml");
        let workspaces = [Workspace::load_foundry(manifest).unwrap()];
        let importer = project.path("/project/src/Main.sol");
        let context = ImportResolutionContext::for_workspaces(&workspaces, &importer).unwrap();

        let resolved = ImportResolver::new(context, &[]).resolve(&importer, "pkg/Target.sol");

        assert_eq!(resolved, Some(project.path("/project/lib/Target.sol")));
    }

    #[test]
    fn resolver_caps_candidates_and_marks_the_result_incomplete() {
        let project = TestProject::new();
        project.write_file("/foundry.toml", "");
        project.write_file("/src/Main.sol", "import \"./\";");
        for index in 0..MAX_IMPORT_CANDIDATES {
            project.write_file(&format!("/src/Candidate{index:03}.sol"), "");
        }
        let config = project.config();
        let importer = project.path("/src/Main.sol");
        let context = config.import_resolution_context(&importer).unwrap();

        let completion = ImportResolver::new(context, &[]).complete(&importer, "./");

        assert_eq!(completion.candidates().len(), MAX_IMPORT_CANDIDATES);
        assert_eq!(completion.candidates()[0].import_path(), "./Candidate000.sol");
        assert_eq!(
            completion.candidates()[MAX_IMPORT_CANDIDATES - 1].import_path(),
            "./Candidate255.sol"
        );
        assert!(completion.is_incomplete());
    }

    #[test]
    fn config_selects_the_deepest_import_resolution_context() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/outer/src/"]

            //- /src/Outer.sol
            contract Outer {}

            //- /packages/app/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/inner/src/"]

            //- /packages/app/src/Inner.sol
            contract Inner {}
            "#,
        );
        let config = project.config();

        let outer = config.import_resolution_context(&project.path("/src/Outer.sol")).unwrap();
        let inner = config
            .import_resolution_context(&project.path("/packages/app/lib/pkg/Overlay.sol"))
            .unwrap();

        assert_eq!(outer.workspace_root(), project.root());
        assert_eq!(inner.workspace_root(), project.path("/packages/app"));
        assert_eq!(
            outer
                .compile_opts()
                .import_remappings
                .iter()
                .find(|remapping| remapping.prefix == "pkg/")
                .unwrap()
                .path,
            "lib/outer/src/"
        );
        assert_eq!(
            inner
                .compile_opts()
                .import_remappings
                .iter()
                .find(|remapping| remapping.prefix == "pkg/")
                .unwrap()
                .path,
            "lib/inner/src/"
        );
    }

    #[test]
    fn config_isolates_import_contexts_across_workspace_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/first/"]

            //- /first/src/Main.sol
            contract First {}

            //- /second/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/second/"]

            //- /second/src/Main.sol
            contract Second {}
            "#,
        );
        let config = project.config_with_roots(&["/first", "/second"]);

        let first = config.import_resolution_context(&project.path("/first/src/Main.sol")).unwrap();
        let second =
            config.import_resolution_context(&project.path("/second/src/Main.sol")).unwrap();

        assert_eq!(first.workspace_root(), project.path("/first"));
        assert_eq!(second.workspace_root(), project.path("/second"));
        assert_eq!(first.compile_opts().import_remappings[0].path, "lib/first/");
        assert_eq!(second.compile_opts().import_remappings[0].path, "lib/second/");
    }

    #[test]
    fn config_owns_external_source_roots_without_falling_back_to_the_first_workspace() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/first/"]

            //- /first/src/Main.sol
            contract First {}

            //- /second/foundry.toml
            [profile.default]
            src = "../shared"
            auto_detect_remappings = false
            remappings = ["pkg/=lib/second/"]

            //- /shared/Main.sol
            contract Shared {}
            "#,
        );
        let config = project.config_with_roots(&["/first", "/second"]);

        let context = config.import_resolution_context(&project.path("/shared/Main.sol")).unwrap();

        assert_eq!(context.workspace_root(), project.path("/second"));
        assert_eq!(context.compile_opts().import_remappings[0].path, "lib/second/");
    }

    #[test]
    fn config_prefers_a_deeper_external_source_root_over_an_ancestor_base_path() {
        let project = TestProject::from_fixture(
            r#"
            //- /outer/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/outer/"]

            //- /outer/packages/app/foundry.toml
            [profile.default]
            src = "../../shared"
            auto_detect_remappings = false
            remappings = ["pkg/=lib/inner/"]

            //- /outer/shared/Main.sol
            contract Shared {}
            "#,
        );
        let config = project.config_with_roots(&["/outer"]);

        let context =
            config.import_resolution_context(&project.path("/outer/shared/Main.sol")).unwrap();

        assert_eq!(context.workspace_root(), project.path("/outer/packages/app"));
        assert_eq!(context.compile_opts().import_remappings[0].path, "lib/inner/");
    }

    #[test]
    fn config_owns_external_import_only_roots_without_cross_project_contamination() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/first/"]

            //- /first/src/Main.sol
            contract First {}

            //- /second/foundry.toml
            [profile.default]
            libs = ["../dependencies"]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/second/"]

            //- /second/src/Main.sol
            contract Second {}

            //- /dependencies/pkg/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = project.config_with_roots(&["/first", "/second"]);

        let context = config
            .import_resolution_context(&project.path("/dependencies/pkg/Overlay.sol"))
            .unwrap();

        assert_eq!(context.workspace_root(), project.path("/second"));
        assert_eq!(context.compile_opts().import_remappings[0].path, "lib/second/");
    }

    #[test]
    fn config_owns_out_of_base_remapping_targets() {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["pkg/=../shared/"]

            //- /project/src/Main.sol
            import "pkg/Dependency.sol";

            //- /shared/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = project.config_with_roots(&["/project"]);

        let context =
            config.import_resolution_context(&project.path("/shared/Dependency.sol")).unwrap();

        assert_eq!(context.workspace_root(), project.path("/project"));
        assert_eq!(context.compile_opts().import_remappings[0].path, "../shared/");
    }

    #[test]
    fn config_does_not_guess_an_import_context_for_an_unowned_path() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml

            //- /first/src/Main.sol
            contract First {}

            //- /second/foundry.toml

            //- /second/src/Main.sol
            contract Second {}
            "#,
        );
        let config = project.config_with_roots(&["/first", "/second"]);

        assert!(config.import_resolution_context(&project.path("/unowned/Overlay.sol")).is_none());
    }

    #[test]
    fn config_rejects_ambiguous_shared_import_contexts() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            libs = ["../shared"]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/first/"]

            //- /second/foundry.toml
            [profile.default]
            libs = ["../shared"]
            auto_detect_remappings = false
            remappings = ["pkg/=lib/second/"]

            //- /shared/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = project.config_with_roots(&["/first", "/second"]);

        assert!(
            config.import_resolution_context(&project.path("/shared/Dependency.sol")).is_none()
        );
    }
}
