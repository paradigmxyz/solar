//! Loading a file to verify together with the files it imports.
//!
//! The verifier resolves names globally, as if the files were flattened into one. [`Loader::load`]
//! parses a file and, recursively, every file it imports, resolving import paths with the
//! compiler's file resolver (remappings, base path and include paths, as `solar` and solc take
//! them), and returns the parsed files with every file after the files it imports. Two readings
//! cannot be flattened, so they are errors that stop the file from being verified: an import
//! that renames or namespaces what it imports (`as`, `* as`), and a name that two files define.

use solar_parse::{
    Parser,
    ast::{self, ImportItems, ItemKind, SourceUnit},
    interface::{
        Session, Span,
        config::ImportRemapping,
        data_structures::map::{FxHashMap, FxHashSet},
        source_map::{FileName, FileResolver, SourceFile},
    },
    unescape::{self, StrKind},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Where imports are looked up, as `solar` and solc take it on the command line.
#[derive(Clone, Debug, Default)]
pub(crate) struct ImportOpts {
    /// `prefix=path` and `context:prefix=path` remappings, as written.
    pub remappings: Vec<String>,
    pub base_path: Option<PathBuf>,
    pub include_paths: Vec<PathBuf>,
}

impl ImportOpts {
    /// Takes the option at `arg`, reading its value from `rest`; false when `arg` is not one.
    pub(crate) fn parse_arg(&mut self, arg: &str, rest: &mut impl Iterator<Item = String>) -> bool {
        match arg {
            "--base-path" => self.base_path = rest.next().map(PathBuf::from),
            "-I" | "--include-path" => self.include_paths.extend(rest.next().map(PathBuf::from)),
            _ if !arg.starts_with('-') && arg.contains('=') => self.remappings.push(arg.into()),
            _ => return false,
        }
        true
    }

    /// The options as arguments, for a child process.
    pub(crate) fn args(&self) -> Vec<String> {
        let mut args = self.remappings.clone();
        if let Some(base) = &self.base_path {
            args.extend(["--base-path".into(), base.display().to_string()]);
        }
        for path in &self.include_paths {
            args.extend(["--include-path".into(), path.display().to_string()]);
        }
        args
    }

    /// The remappings, or the first one that does not parse.
    pub(crate) fn parsed_remappings(&self) -> Result<Vec<ImportRemapping>, String> {
        self.remappings
            .iter()
            .map(|remapping| {
                remapping
                    .parse()
                    .map_err(|error| format!("invalid remapping `{remapping}`: {error}"))
            })
            .collect()
    }
}

/// A file to verify, parsed with the files it imports.
pub(crate) struct Loaded<'a> {
    /// The file to verify.
    pub file: Arc<SourceFile>,
    /// The parsed files, each after the files it imports, so the file to verify is last.
    pub units: Vec<&'a SourceUnit<'a>>,
}

/// Parses files and the files they import.
pub(crate) struct Loader<'s, 'a> {
    sess: &'s Session,
    arena: &'a ast::Arena,
    resolver: FileResolver<'s>,
}

impl<'s, 'a> Loader<'s, 'a> {
    pub(crate) fn new(
        sess: &'s Session,
        arena: &'a ast::Arena,
        opts: &ImportOpts,
    ) -> Result<Self, String> {
        let mut resolver = FileResolver::new(sess.source_map());
        resolver.add_import_remappings(opts.parsed_remappings()?);
        if let Some(base) = &opts.base_path {
            resolver.set_base_path(base);
        }
        resolver.add_include_paths(opts.include_paths.iter().cloned());
        Ok(Self { sess, arena, resolver })
    }

    /// `file` and everything it imports. With `mutant`, `file` holds what `mutant` holds instead
    /// of what is on disk, and imports resolve from where `file` is: this is how mutation testing
    /// verifies a mutant. Diagnostics are emitted; `Err` means they were.
    pub(crate) fn load(&self, file: &Path, mutant: Option<&Path>) -> Result<Loaded<'a>, ()> {
        let source_map = self.sess.source_map();
        let root = match mutant {
            Some(mutant) => source_map
                .file_loader()
                .load_file(mutant)
                .and_then(|text| source_map.new_source_file(FileName::Real(file.into()), text))
                .map_err(|error| error.to_string()),
            None => self.resolver.resolve_file(file, None).map_err(|error| error.to_string()),
        };
        let root = root.map_err(|error| {
            self.sess.dcx.err(format!("cannot read `{}`: {error}", file.display())).emit();
        })?;
        let mut load = Load { loader: self, seen: FxHashSet::default(), units: vec![], ok: true };
        load.visit(root.clone());
        let Load { units, ok, .. } = load;
        let units: Vec<_> = units.into_iter().map(|(_, unit)| unit).collect();
        let unique = self.check_unique_names(&units);
        if !ok {
            return Err(());
        }
        unique?;
        Ok(Loaded { file: root, units })
    }

    /// Reports names that two files define: resolving names globally would pick one of them.
    fn check_unique_names(&self, units: &[&'a SourceUnit<'a>]) -> Result<(), ()> {
        let mut defined: FxHashMap<String, (usize, Span, Span)> = FxHashMap::default();
        let mut ok = true;
        for (index, unit) in units.iter().enumerate() {
            // File-level names, and the types contracts declare, which are looked up by bare name
            // too.
            let nested = unit.items.iter().flat_map(|item| match &item.kind {
                ItemKind::Contract(contract) => &contract.body[..],
                _ => &[],
            });
            let types = nested.filter(|item| {
                matches!(item.kind, ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Udvt(_))
            });
            for item in unit.items.iter().chain(types) {
                let Some(name) = item.name() else { continue };
                let defines = matches!(
                    item.kind,
                    ItemKind::Contract(_)
                        | ItemKind::Function(_)
                        | ItemKind::Variable(_)
                        | ItemKind::Struct(_)
                        | ItemKind::Enum(_)
                        | ItemKind::Udvt(_)
                        | ItemKind::Error(_)
                        | ItemKind::Event(_)
                );
                if !defines {
                    continue;
                }
                match defined.entry(name.as_str().to_string()) {
                    // Identical constants (same text, such as a constant two files of a
                    // library both declare) mean the same thing wherever they resolve.
                    std::collections::hash_map::Entry::Occupied(first)
                        if first.get().0 != index
                            && matches!(item.kind, ItemKind::Variable(_))
                            && self
                                .sess
                                .source_map()
                                .span_to_snippet(item.span)
                                .ok()
                                .is_some_and(|a| {
                                    self.sess.source_map().span_to_snippet(first.get().2).ok()
                                        == Some(a)
                                }) => {}
                    std::collections::hash_map::Entry::Occupied(first)
                        if first.get().0 != index =>
                    {
                        self.sess
                            .dcx
                            .err(format!("`{name}` is defined in more than one file"))
                            .span(name.span)
                            .span_note(first.get().1, "also defined here")
                            .note("solar-verify resolves names as if the files were flattened")
                            .emit();
                        ok = false;
                    }
                    std::collections::hash_map::Entry::Occupied(_) => {}
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert((index, name.span, item.span));
                    }
                }
            }
        }
        if ok { Ok(()) } else { Err(()) }
    }
}

/// One traversal of a file's imports.
struct Load<'l, 's, 'a> {
    loader: &'l Loader<'s, 'a>,
    seen: FxHashSet<Arc<SourceFile>>,
    /// The parsed files, each after the files it imports.
    units: Vec<(Arc<SourceFile>, &'a SourceUnit<'a>)>,
    ok: bool,
}

impl<'s, 'a> Load<'_, 's, 'a> {
    fn visit(&mut self, file: Arc<SourceFile>) {
        if !self.seen.insert(file.clone()) {
            return;
        }
        let sess = self.loader.sess;
        let mut parser = Parser::from_source_file(sess, self.loader.arena, &file);
        let unit = match parser.parse_file() {
            Ok(unit) => &*self.loader.arena.alloc(unit),
            Err(error) => {
                error.emit();
                self.ok = false;
                return;
            }
        };
        let parent = match &file.name {
            FileName::Real(path) => path.as_path(),
            _ => Path::new(""),
        };
        for item in unit.items.iter() {
            let ItemKind::Import(import) = &item.kind else { continue };
            if !self.check_flattenable(import, item.span) {
                continue;
            }
            if let Some(imported) = self.resolve(import, parent) {
                self.visit(imported);
            }
        }
        self.units.push((file, unit));
    }

    /// Whether the import brings names in as they are; renames and namespaces are reported.
    fn check_flattenable(&mut self, import: &ast::ImportDirective<'_>, span: Span) -> bool {
        let renamed = match &import.items {
            ImportItems::Plain(alias) => alias.is_some(),
            ImportItems::Aliases(aliases) => aliases.iter().any(|(_, alias)| alias.is_some()),
            ImportItems::Glob(_) => true,
        };
        if renamed {
            self.loader
                .sess
                .dcx
                .err("imports that rename or namespace symbols are not supported")
                .span(span)
                .help("import the names as they are: `import {A} from \"a.sol\";`")
                .emit();
            self.ok = false;
        }
        !renamed
    }

    fn resolve(
        &mut self,
        import: &ast::ImportDirective<'_>,
        parent: &Path,
    ) -> Option<Arc<SourceFile>> {
        let sess = self.loader.sess;
        let span = import.path.span;
        let (bytes, any_error) =
            unescape::parse_string_literal(import.path.value.as_str(), StrKind::Str, span, sess);
        let path = (!any_error).then(|| std::str::from_utf8(&bytes).ok()).flatten();
        let Some(path) = path else {
            sess.dcx.err("import path is not a valid UTF-8 string").span(span).emit();
            self.ok = false;
            return None;
        };
        match self.loader.resolver.resolve_file(Path::new(path), Some(parent)) {
            Ok(file) => Some(file),
            Err(error) => {
                sess.dcx
                    .err(error.to_string())
                    .span(span)
                    .help("pass remappings as `prefix=path`, or `--base-path` and `--include-path`")
                    .emit();
                self.ok = false;
                None
            }
        }
    }
}
